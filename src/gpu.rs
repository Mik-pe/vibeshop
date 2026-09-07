use crate::document::{Blend, Document, Layer};
use anyhow::{Context, Result, ensure};
use bytemuck::{Pod, Zeroable};
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc,
    time::Duration,
};
use wgpu::util::DeviceExt;

pub const TILE_SIZE: u32 = 512;

// Only pixel-affecting state is retained; names and old source assets are not.
#[derive(Clone, PartialEq)]
struct LayerKey {
    source: u64,
    tone: [f32; 4],
    offset: [i32; 2],
    blend: Blend,
}
impl From<&Layer> for LayerKey {
    fn from(layer: &Layer) -> Self {
        Self {
            source: layer.source.id,
            tone: [
                layer.exposure,
                layer.contrast,
                layer.saturation,
                layer.opacity,
            ],
            offset: layer.offset,
            blend: layer.blend,
        }
    }
}
fn intersects(layer: &Layer, x: u32, y: u32, width: u32, height: u32) -> bool {
    let [lx, ly] = layer.offset.map(i64::from);
    layer.visible
        && layer.opacity > 0.0
        && lx < i64::from(x + width)
        && ly < i64::from(y + height)
        && lx + i64::from(layer.source.width) > i64::from(x)
        && ly + i64::from(layer.source.height) > i64::from(y)
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Parameters {
    tone: [f32; 4],
    offset: [i32; 2],
    blend: u32,
    clear_backdrop: u32,
}

struct Surface {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}
impl Surface {
    fn new(device: &wgpu::Device, width: u32, height: u32, format: wgpu::TextureFormat) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Vibeshop image"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST
                | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());
        Self { texture, view }
    }
}
struct Targets {
    width: u32,
    height: u32,
    scratch: [Surface; 2],
    display: Surface,
    export: Surface,
    tiles: Vec<Vec<LayerKey>>,
}

pub struct Engine {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    composite: wgpu::ComputePipeline,
    encode: wgpu::ComputePipeline,
    sources: HashMap<u64, Surface>,
    targets: Option<Targets>,
    pub uploads: u64,
    pub renders: u64,
    pub tiles_rendered: u64,
    render_valid: bool,
}
impl Engine {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/composite.wgsl"));
        let composite = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Linear-light layers"),
            layout: None,
            module: &shader,
            entry_point: Some("composite"),
            compilation_options: Default::default(),
            cache: None,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/encode.wgsl"));
        let encode = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("sRGB display and export"),
            layout: None,
            module: &shader,
            entry_point: Some("encode"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            device,
            queue,
            composite,
            encode,
            sources: HashMap::new(),
            targets: None,
            uploads: 0,
            renders: 0,
            tiles_rendered: 0,
            render_valid: false,
        }
    }
    pub fn render(&mut self, document: &Document) -> Result<bool> {
        let previous_valid = self.render_valid;
        self.render_valid = false;
        document.validate()?;
        let limit = self.device.limits().max_texture_dimension_2d;
        ensure!(
            document.width <= limit
                && document.height <= limit
                && document
                    .layers
                    .iter()
                    .all(|l| l.source.width <= limit && l.source.height <= limit),
            "Image exceeds this GPU's texture limit ({limit}px)"
        );
        let resized = self
            .targets
            .as_ref()
            .is_none_or(|t| t.width != document.width || t.height != document.height);
        if resized {
            self.targets = None;
            let make = |format| Surface::new(&self.device, document.width, document.height, format);
            self.targets = Some(Targets {
                width: document.width,
                height: document.height,
                scratch: [
                    Surface::new(
                        &self.device,
                        TILE_SIZE,
                        TILE_SIZE,
                        wgpu::TextureFormat::Rgba16Float,
                    ),
                    Surface::new(
                        &self.device,
                        TILE_SIZE,
                        TILE_SIZE,
                        wgpu::TextureFormat::Rgba16Float,
                    ),
                ],
                display: make(wgpu::TextureFormat::Rgba8Unorm),
                export: make(wgpu::TextureFormat::Rgba8Unorm),
                tiles: vec![
                    Vec::new();
                    (document.width.div_ceil(TILE_SIZE) * document.height.div_ceil(TILE_SIZE))
                        as usize
                ],
            });
        }
        let active: HashSet<_> = document.layers.iter().map(|l| l.source.id).collect();
        self.sources.retain(|id, _| active.contains(id));
        for layer in &document.layers {
            if let std::collections::hash_map::Entry::Vacant(entry) =
                self.sources.entry(layer.source.id)
            {
                let s = &layer.source;
                let surface = Surface::new(
                    &self.device,
                    s.width,
                    s.height,
                    wgpu::TextureFormat::Rgba8Unorm,
                );
                self.queue.write_texture(
                    surface.texture.as_image_copy(),
                    &s.rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(s.width * 4),
                        rows_per_image: Some(s.height),
                    },
                    wgpu::Extent3d {
                        width: s.width,
                        height: s.height,
                        depth_or_array_layers: 1,
                    },
                );
                entry.insert(surface);
                self.uploads += 1;
            }
        }
        let t = self.targets.as_mut().context("No GPU targets")?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        let mut changed = 0;
        for y in (0..t.height).step_by(TILE_SIZE as usize) {
            for x in (0..t.width).step_by(TILE_SIZE as usize) {
                let width = TILE_SIZE.min(t.width - x);
                let height = TILE_SIZE.min(t.height - y);
                let layers: Vec<_> = document
                    .layers
                    .iter()
                    .filter(|l| intersects(l, x, y, width, height))
                    .collect();
                let keys: Vec<_> = layers.iter().map(|l| LayerKey::from(*l)).collect();
                let index = (y / TILE_SIZE * t.width.div_ceil(TILE_SIZE) + x / TILE_SIZE) as usize;
                if previous_valid && !resized && t.tiles[index] == keys {
                    continue;
                }
                changed += 1;
                let mut current = 0;
                let empty = layers.is_empty();
                for (layer_index, layer) in layers.into_iter().enumerate() {
                    let parameters = Parameters {
                        tone: [
                            layer.exposure,
                            layer.contrast,
                            layer.saturation,
                            layer.opacity,
                        ],
                        offset: [layer.offset[0] - x as i32, layer.offset[1] - y as i32],
                        blend: layer.blend as u32,
                        clear_backdrop: u32::from(layer_index == 0),
                    };
                    let uniform =
                        self.device
                            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                                label: Some("Layer adjustments"),
                                contents: bytemuck::bytes_of(&parameters),
                                usage: wgpu::BufferUsages::UNIFORM,
                            });
                    let source = &self.sources[&layer.source.id];
                    let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Layer inputs"),
                        layout: &self.composite.get_bind_group_layout(0),
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: wgpu::BindingResource::TextureView(&source.view),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(
                                    &t.scratch[current].view,
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 2,
                                resource: wgpu::BindingResource::TextureView(
                                    &t.scratch[1 - current].view,
                                ),
                            },
                            wgpu::BindGroupEntry {
                                binding: 3,
                                resource: uniform.as_entire_binding(),
                            },
                        ],
                    });
                    dispatch(&mut encoder, &self.composite, &bind, width, height);
                    current = 1 - current;
                }
                let origin = self
                    .device
                    .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("Tile output origin"),
                        contents: bytemuck::cast_slice(&[x, y, u32::from(empty), 0]),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Display and export"),
                    layout: &self.encode.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&t.scratch[current].view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(&t.display.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::TextureView(&t.export.view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: origin.as_entire_binding(),
                        },
                    ],
                });
                dispatch(&mut encoder, &self.encode, &bind, width, height);
                t.tiles[index] = keys;
            }
        }
        if changed > 0 {
            self.queue.submit([encoder.finish()]);
            self.renders += 1;
            self.tiles_rendered += changed;
        }
        self.render_valid = true;
        Ok(resized)
    }
    pub fn display_view(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|t| &t.display.view)
    }
    pub fn readback(&self) -> Result<Readback> {
        ensure!(
            self.render_valid,
            "The current image could not be rendered; refusing to export stale pixels"
        );
        let t = self
            .targets
            .as_ref()
            .context("Render an image before exporting")?;
        let stride = (t.width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Export snapshot"),
            size: u64::from(stride) * u64::from(t.height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            t.export.texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(stride),
                    rows_per_image: Some(t.height),
                },
            },
            wgpu::Extent3d {
                width: t.width,
                height: t.height,
                depth_or_array_layers: 1,
            },
        );
        let submission = self.queue.submit([encoder.finish()]);
        Ok(Readback {
            device: self.device.clone(),
            buffer,
            submission,
            width: t.width,
            height: t.height,
            stride,
        })
    }
}
fn dispatch(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::ComputePipeline,
    bind: &wgpu::BindGroup,
    width: u32,
    height: u32,
) {
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("Photo operation"),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind, &[]);
    pass.dispatch_workgroups(width.div_ceil(8), height.div_ceil(8), 1);
}

pub struct Readback {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
    pub width: u32,
    pub height: u32,
    stride: u32,
}
impl Readback {
    // Call from an IO worker or test, never from the interactive UI thread.
    pub fn finish(self) -> Result<Vec<u8>> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(self.submission),
            timeout: Some(Duration::from_secs(30)),
        })?;
        rx.recv_timeout(Duration::from_secs(30))
            .context("GPU export timed out")??;
        let mapped = self.buffer.slice(..).get_mapped_range();
        let mut pixels = Vec::with_capacity((self.width * self.height * 4) as usize);
        for row in mapped.chunks_exact(self.stride as usize) {
            pixels.extend_from_slice(&row[..self.width as usize * 4]);
        }
        drop(mapped);
        self.buffer.unmap();
        Ok(pixels)
    }
}

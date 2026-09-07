use crate::curves::{Curve, Levels, levels_uniform};
use crate::document::{Blend, Document, Layer};
use anyhow::{Context, Result, ensure};
use bytemuck::{Pod, Zeroable};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    sync::mpsc,
    time::Duration,
};
use wgpu::util::DeviceExt;

pub const TILE_SIZE: u32 = 512;
/// Maximum mapped transfer storage for one export, independent of image height.
pub const EXPORT_STAGING_BYTES: u64 = 1024 * 1024;

// Only pixel-affecting state is retained; names and old source assets are not.
#[derive(Clone, PartialEq)]
struct LayerKey {
    source: u64,
    tone: [f32; 4],
    offset: [i32; 2],
    blend: Blend,
    levels: Levels,
    curves: [Curve; 4],
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
            levels: layer.levels,
            curves: layer.curves.clone(),
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
    levels: [f32; 3],
    curve_mask: u32,
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
    histogram_tiles: wgpu::Buffer,
    histogram: wgpu::Buffer,
}

pub struct Engine {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    composite: wgpu::ComputePipeline,
    encode: wgpu::ComputePipeline,
    histogram_reduce: wgpu::ComputePipeline,
    curve_luts: Vec<([Curve; 4], wgpu::Texture)>,
    compare: bool,
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
        let reduce_shader =
            device.create_shader_module(wgpu::include_wgsl!("shaders/histogram_reduce.wgsl"));
        let histogram_reduce = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Histogram total"),
            layout: None,
            module: &reduce_shader,
            entry_point: Some("reduce"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            device,
            queue,
            composite,
            encode,
            histogram_reduce,
            curve_luts: Vec::new(),
            compare: false,
            sources: HashMap::new(),
            targets: None,
            uploads: 0,
            renders: 0,
            tiles_rendered: 0,
            render_valid: false,
        }
    }
    pub fn set_compare(&mut self, compare: bool) {
        if self.compare != compare {
            self.compare = compare;
            self.render_valid = false;
        }
    }
    pub fn render_valid(&self) -> bool {
        self.render_valid
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
                histogram_tiles: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Cached tile histograms"),
                    size: u64::from(
                        document.width.div_ceil(TILE_SIZE) * document.height.div_ceil(TILE_SIZE),
                    ) * 4096,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                histogram: self.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Histogram total"),
                    size: 4096,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
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
        self.curve_luts.truncate(document.layers.len());
        for (index, layer) in document.layers.iter().enumerate() {
            if self
                .curve_luts
                .get(index)
                .is_some_and(|(curves, _)| *curves == layer.curves)
            {
                continue;
            }
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Layer curve LUT"),
                size: wgpu::Extent3d {
                    width: 256,
                    height: 4,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::R32Float,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let values: Vec<f32> = layer
                .curves
                .iter()
                .flat_map(|curve| (0..256).map(move |x| curve.eval(x as f32 / 255.0)))
                .collect();
            self.queue.write_texture(
                texture.as_image_copy(),
                bytemuck::cast_slice(&values),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(1024),
                    rows_per_image: Some(4),
                },
                wgpu::Extent3d {
                    width: 256,
                    height: 4,
                    depth_or_array_layers: 1,
                },
            );
            if index == self.curve_luts.len() {
                self.curve_luts.push((layer.curves.clone(), texture));
            } else {
                self.curve_luts[index] = (layer.curves.clone(), texture);
            }
        }
        let lut_views: Vec<_> = self
            .curve_luts
            .iter()
            .map(|(_, texture)| texture.create_view(&Default::default()))
            .collect();
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
                    .enumerate()
                    .filter(|(_, l)| intersects(l, x, y, width, height))
                    .collect();
                let keys: Vec<_> = layers.iter().map(|(_, l)| LayerKey::from(*l)).collect();
                let index = (y / TILE_SIZE * t.width.div_ceil(TILE_SIZE) + x / TILE_SIZE) as usize;
                if previous_valid && !resized && t.tiles[index] == keys {
                    continue;
                }
                changed += 1;
                let mut current = 0;
                let empty = layers.is_empty();
                for (layer_index, (document_index, layer)) in layers.into_iter().enumerate() {
                    let parameters = Parameters {
                        tone: [
                            if self.compare { 0.0 } else { layer.exposure },
                            if self.compare { 1.0 } else { layer.contrast },
                            if self.compare { 1.0 } else { layer.saturation },
                            layer.opacity,
                        ],
                        offset: [layer.offset[0] - x as i32, layer.offset[1] - y as i32],
                        blend: layer.blend as u32,
                        clear_backdrop: u32::from(layer_index == 0),
                        levels: levels_uniform(&if self.compare {
                            Levels::default()
                        } else {
                            layer.levels
                        }),
                        curve_mask: if self.compare {
                            0
                        } else {
                            layer
                                .curves
                                .iter()
                                .enumerate()
                                .fold(0, |mask, (bit, curve)| {
                                    mask | (u32::from(!curve.is_neutral()) << bit)
                                })
                        },
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
                            wgpu::BindGroupEntry {
                                binding: 4,
                                resource: wgpu::BindingResource::TextureView(
                                    &lut_views[document_index],
                                ),
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
                        contents: bytemuck::cast_slice(&[
                            x,
                            y,
                            u32::from(empty),
                            index as u32 * 1024,
                        ]),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                // Replacing an empty tile must also erase its previous bin counts.
                encoder.clear_buffer(&t.histogram_tiles, index as u64 * 4096, Some(4096));
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
                        wgpu::BindGroupEntry {
                            binding: 4,
                            resource: t.histogram_tiles.as_entire_binding(),
                        },
                    ],
                });
                dispatch(
                    &mut encoder,
                    &self.encode,
                    &bind,
                    width.div_ceil(4),
                    height.div_ceil(4),
                );
                t.tiles[index] = keys;
            }
        }
        if changed > 0 {
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Histogram reduction"),
                layout: &self.histogram_reduce.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: t.histogram_tiles.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: t.histogram.as_entire_binding(),
                    },
                ],
            });
            dispatch(&mut encoder, &self.histogram_reduce, &bind, 256, 4);
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
        ensure!(!self.compare, "Switch to edited pixels before exporting");
        ensure!(
            self.render_valid,
            "The current image could not be rendered; refusing to export stale pixels"
        );
        let t = self
            .targets
            .as_ref()
            .context("Render an image before exporting")?;
        // Freeze the requested revision before a file dialog or later edits. A texture
        // handle alone would alias the live output; bands must all see this snapshot.
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Immutable export snapshot"),
            size: t.export.texture.size(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_texture(
            t.export.texture.as_image_copy(),
            texture.as_image_copy(),
            texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        Ok(Readback {
            device: self.device.clone(),
            queue: self.queue.clone(),
            texture,
        })
    }
    /// Copy out the 1024-bin histogram computed during the last render.
    /// One 4 KiB readback: rows are luminance, R, G, B as u32 counts.
    pub fn histogram(&self) -> Result<HistogramReadback> {
        ensure!(
            self.render_valid,
            "Render an image before reading its histogram"
        );
        let buffer = &self
            .targets
            .as_ref()
            .context("No histogram has been computed yet")?
            .histogram;
        let staging = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Tone histogram readback"),
            size: 1024 * 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, None);
        let submission = self.queue.submit([encoder.finish()]);
        Ok(HistogramReadback {
            device: self.device.clone(),
            buffer: staging,
            submission,
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
    queue: wgpu::Queue,
    texture: wgpu::Texture,
}
impl Readback {
    pub fn dimensions(&self) -> (u32, u32) {
        (self.texture.width(), self.texture.height())
    }

    /// Stream straight-alpha RGBA8 rows from the frozen revision. Call on an IO
    /// worker: each band waits for the GPU and is consumed before reusing storage.
    pub fn write_to(self, writer: &mut impl Write) -> Result<()> {
        let (width, height) = self.dimensions();
        let stride = (width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
            * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let rows = (EXPORT_STAGING_BYTES / u64::from(stride)).min(u64::from(height)) as u32;
        ensure!(rows > 0, "Image row exceeds the export staging budget");
        let size = u64::from(stride) * u64::from(rows);
        ensure!(
            size <= self.device.limits().max_buffer_size,
            "Export staging exceeds device limits"
        );
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Bounded export band"),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        for y in (0..height).step_by(rows as usize) {
            let count = rows.min(height - y);
            let mut encoder = self.device.create_command_encoder(&Default::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    origin: wgpu::Origin3d { x: 0, y, z: 0 },
                    ..self.texture.as_image_copy()
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(stride),
                        rows_per_image: Some(count),
                    },
                },
                wgpu::Extent3d {
                    width,
                    height: count,
                    depth_or_array_layers: 1,
                },
            );
            let submission = self.queue.submit([encoder.finish()]);
            let (tx, rx) = mpsc::sync_channel(1);
            buffer
                .slice(..)
                .map_async(wgpu::MapMode::Read, move |result| {
                    let _ = tx.send(result);
                });
            self.device.poll(wgpu::PollType::Wait {
                submission_index: Some(submission),
                timeout: Some(Duration::from_secs(30)),
            })?;
            rx.recv_timeout(Duration::from_secs(30))
                .context("GPU export timed out")??;
            let result = (|| -> Result<()> {
                let mapped = buffer.slice(..).get_mapped_range();
                for row in mapped.chunks_exact(stride as usize).take(count as usize) {
                    writer.write_all(&row[..width as usize * 4])?;
                }
                Ok(())
            })();
            // Release mapping even when the encoder/destination rejects a row.
            buffer.unmap();
            result?;
        }
        Ok(())
    }

    /// Collect pixels for callers that explicitly need them (e.g. reference tests).
    /// Production PNG export streams through [`Self::write_to`] instead.
    pub fn finish(self) -> Result<Vec<u8>> {
        let mut pixels = Vec::new();
        let (width, height) = self.dimensions();
        pixels.try_reserve_exact(width as usize * height as usize * 4)?;
        self.write_to(&mut pixels)?;
        Ok(pixels)
    }
}

/// 4x256 u32 bin counts: luminance, red, green, blue.
pub type HistogramData = [[u32; 256]; 4];

pub struct HistogramReadback {
    device: wgpu::Device,
    buffer: wgpu::Buffer,
    submission: wgpu::SubmissionIndex,
}
impl HistogramReadback {
    /// Hand the readback to a background worker; call [`Self::finish`] there.
    pub fn spawn(self) -> std::sync::mpsc::Receiver<Result<HistogramData>> {
        let (tx, rx) = mpsc::sync_channel(1);
        std::thread::spawn(move || {
            let _ = tx.send(self.finish());
        });
        rx
    }
    // Same discipline as export readback: never block the UI thread.
    pub fn finish(self) -> Result<HistogramData> {
        let (tx, rx) = mpsc::sync_channel(1);
        self.buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = tx.send(result);
            });
        self.device.poll(wgpu::PollType::Wait {
            submission_index: Some(self.submission),
            timeout: Some(Duration::from_secs(10)),
        })?;
        rx.recv_timeout(Duration::from_secs(10))
            .context("Histogram readback timed out")??;
        let mapped = self.buffer.slice(..).get_mapped_range();
        let mut rows = [[0_u32; 256]; 4];
        for (row, values) in rows.iter_mut().enumerate() {
            let bytes: &[u8] = &mapped[row * 256 * 4..(row + 1) * 256 * 4];
            for (bin, value) in values.iter_mut().enumerate() {
                let offset = bin * 4;
                *value = u32::from_le_bytes([
                    bytes[offset],
                    bytes[offset + 1],
                    bytes[offset + 2],
                    bytes[offset + 3],
                ]);
            }
        }
        drop(mapped);
        self.buffer.unmap();
        Ok(rows)
    }
}

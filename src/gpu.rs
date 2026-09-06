use crate::curves::{CURVE_POINTS, levels_uniform};
use crate::document::Document;
use anyhow::{Context, Result, ensure};
use bytemuck::{Pod, Zeroable};
use std::{
    collections::{HashMap, HashSet},
    sync::mpsc,
    time::Duration,
};
use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Parameters {
    tone: [f32; 4],
    offset: [i32; 2],
    blend: u32,
    padding: u32,
    // Linear-light master levels: (input black, input white, 1/gamma).
    levels: [f32; 3],
    // Bit 0 RGB, 1 R, 2 G, 3 B.
    curve_mask: u32,
}

/// Rows: 0 RGB, 1 R, 2 G, 3 B. 256 columns of pre-evaluated curve output.
const CURVE_ROWS: u32 = 4;
const CURVE_LUT_WIDTH: u32 = 256;

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
}

pub struct Engine {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    composite: wgpu::ComputePipeline,
    encode: wgpu::ComputePipeline,
    histogram: wgpu::ComputePipeline,
    sources: HashMap<u64, Surface>,
    targets: Option<Targets>,
    curve_lut: Option<(wgpu::Texture, [u32; 4])>,
    histogram_buffer: Option<wgpu::Buffer>,
    /// Before/after comparison: render every layer with tonal adjustments
    /// bypassed. Render-time only; never mutates the document.
    compare: bool,
    pub uploads: u64,
    pub renders: u64,
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/histogram.wgsl"));
        let histogram = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Tone histogram"),
            layout: None,
            module: &shader,
            entry_point: Some("histogram_pass"),
            compilation_options: Default::default(),
            cache: None,
        });
        Self {
            device,
            queue,
            composite,
            encode,
            histogram,
            sources: HashMap::new(),
            targets: None,
            curve_lut: None,
            histogram_buffer: None,
            compare: false,
            uploads: 0,
            renders: 0,
            render_valid: false,
        }
    }
    /// Show the unadjusted sources instead of the edited composition.
    pub fn set_compare(&mut self, compare: bool) {
        if self.compare != compare {
            self.compare = compare;
            self.render_valid = false;
        }
    }
    pub fn render(&mut self, document: &Document) -> Result<bool> {
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
                    make(wgpu::TextureFormat::Rgba16Float),
                    make(wgpu::TextureFormat::Rgba16Float),
                ],
                display: make(wgpu::TextureFormat::Rgba8Unorm),
                export: make(wgpu::TextureFormat::Rgba8Unorm),
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
        let neutral_curves: [crate::curves::Curve; 4] = Default::default();
        // Pre-upload every curve LUT before borrowing the target views:
        // upload_curve_lut needs &mut self, the bind groups need &self.
        let mut lut_views = Vec::with_capacity(document.layers.len());
        for layer in document
            .layers
            .iter()
            .filter(|l| l.visible && l.opacity > 0.0)
        {
            let uses_curves = layer.curves.iter().any(|curve| !curve.is_neutral());
            let curves = if uses_curves {
                &layer.curves
            } else {
                &neutral_curves
            };
            lut_views.push(self.upload_curve_lut(curves));
        }
        let t = self.targets.as_ref().context("No GPU targets")?;
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let _clear = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Clear composition"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.scratch[0].view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
        }
        let mut current = 0;
        for (layer_index, layer) in document
            .layers
            .iter()
            .filter(|l| l.visible && l.opacity > 0.0)
            .enumerate()
        {
            let mut curve_mask = 0_u32;
            for (bit, curve) in layer.curves.iter().enumerate() {
                if !curve.is_neutral() {
                    curve_mask |= 1 << bit;
                }
            }
            // Before/after comparison bypasses every tonal adjustment.
            let (levels, curve_mask) = if self.compare {
                ([0.0, 1.0, 1.0], 0)
            } else {
                (levels_uniform(&layer.levels), curve_mask)
            };
            let parameters = Parameters {
                tone: [
                    layer.exposure,
                    layer.contrast,
                    layer.saturation,
                    layer.opacity,
                ],
                offset: layer.offset,
                blend: layer.blend as u32,
                padding: 0,
                levels,
                curve_mask,
            };
            let uniform = self
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Layer adjustments"),
                    contents: bytemuck::bytes_of(&parameters),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            // The LUT for this layer was uploaded before the targets borrow.
            let lut_view = &lut_views[layer_index];
            let source = &self.sources[&layer.source.id];
            let mut entries = vec![
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&source.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&t.scratch[current].view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&t.scratch[1 - current].view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: uniform.as_entire_binding(),
                },
            ];
            entries.push(wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(lut_view),
            });
            let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Layer inputs"),
                layout: &self.composite.get_bind_group_layout(0),
                entries: &entries,
            });
            dispatch(&mut encoder, &self.composite, &bind, t.width, t.height);
            current = 1 - current;
        }
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
            ],
        });
        dispatch(&mut encoder, &self.encode, &bind, t.width, t.height);
        // Bump the histogram along the same submission: it reads the final
        // linear-light composition the encode pass just consumed.
        let buffer = self.histogram_buffer.get_or_insert_with(|| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Tone histogram"),
                size: 1024 * 4,
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        });
        encoder.clear_buffer(buffer, 0, None);
        let bind = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Histogram input"),
            layout: &self.histogram.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&t.scratch[current].view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: buffer.as_entire_binding(),
                },
            ],
        });
        dispatch(&mut encoder, &self.histogram, &bind, t.width, t.height);
        self.queue.submit([encoder.finish()]);
        self.renders += 1;
        self.render_valid = true;
        Ok(resized)
    }
    /// Upload (or reuse) the 256x4 curve LUT for one layer.
    fn upload_curve_lut(&mut self, curves: &[crate::curves::Curve; 4]) -> wgpu::TextureView {
        let mut fingerprint = [0_u32; 4];
        let mut data = Vec::with_capacity((CURVE_ROWS * CURVE_LUT_WIDTH * 4) as usize);
        for (row, curve) in curves.iter().enumerate() {
            for index in 0..CURVE_POINTS {
                if let Some(value) = curve.get(index) {
                    let bits = (value.to_bits()).rotate_left((index % 31) as u32);
                    fingerprint[row] = fingerprint[row].wrapping_mul(0x9E3779B1).wrapping_add(bits);
                }
            }
            if curve.is_neutral() {
                // Identity ramp rows still exist so sampling stays defined.
                for x in 0..CURVE_LUT_WIDTH {
                    let v = x as f32 / 255.0;
                    data.extend_from_slice(&v.to_ne_bytes());
                }
                continue;
            }
            for x in 0..CURVE_LUT_WIDTH {
                let v = curve.eval(x as f32 / 255.0);
                data.extend_from_slice(&v.to_ne_bytes());
            }
        }
        let reused = self
            .curve_lut
            .as_ref()
            .is_some_and(|(_, seen)| *seen == fingerprint);
        if !reused {
            let format = wgpu::TextureFormat::R32Float;
            let texture = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Curve LUT"),
                size: wgpu::Extent3d {
                    width: CURVE_LUT_WIDTH,
                    height: CURVE_ROWS,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            // One row per copy: bytes_per_row must be a multiple of 256.
            for row in 0..CURVE_ROWS {
                let start = (row * CURVE_LUT_WIDTH * 4) as usize;
                self.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d { x: 0, y: row, z: 0 },
                        aspect: wgpu::TextureAspect::All,
                    },
                    &data[start..start + (CURVE_LUT_WIDTH * 4) as usize],
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(CURVE_LUT_WIDTH * 4),
                        rows_per_image: Some(1),
                    },
                    wgpu::Extent3d {
                        width: CURVE_LUT_WIDTH,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                );
            }
            let view = texture.create_view(&Default::default());
            self.curve_lut = Some((texture, fingerprint));
            return view;
        }
        let (texture, _) = self.curve_lut.as_ref().unwrap();
        texture.create_view(&Default::default())
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
    /// Copy out the 1024-bin histogram computed during the last render.
    /// One 4 KiB readback: rows are luminance, R, G, B as u32 counts.
    pub fn histogram(&self) -> Result<HistogramReadback> {
        ensure!(
            self.render_valid,
            "Render an image before reading its histogram"
        );
        let buffer = self
            .histogram_buffer
            .as_ref()
            .context("No histogram has been computed yet")?;
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
    /// True when a render has completed and its outputs are current.
    pub fn render_valid(&self) -> bool {
        self.render_valid
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

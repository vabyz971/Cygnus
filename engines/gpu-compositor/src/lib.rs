// Cygnus — Professional open-source creative suite
// Copyright (C) 2026 vabyz971
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! GPU layer compositor (wgpu compute), domain-free.
//!
//! Blends an ordered list of layers — texture, affine placement,
//! opacity, blend mode — into a target texture, entirely on the GPU, on
//! whatever device the caller provides (the app's eframe device, so the
//! result is displayed without any CPU readback). Shared by photo today,
//! video tomorrow (clips are layers).
//!
//! Pipeline per frame:
//! 1. clear the accumulator (transparent);
//! 2. one compute pass per layer, ping-ponging two accumulators
//!    (`composite.wgsl`: straight alpha, gamma space, CPU-identical math
//!    and 8-bit truncation);
//! 3. present: premultiply into the output texture (egui's convention)
//!    and build its mip chain (smooth zoomed-out display).
//!
//! All passes go into ONE submission: a reader on the same queue sees
//! either the previous complete frame or the new one, never a partial.

use wgpu::util::DeviceExt;

/// Workgroup edge of every compute shader.
const WORKGROUP: u32 = 8;

/// Texture format of layers, accumulators and output.
pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// A layer uploaded to the GPU (straight-alpha RGBA8).
pub struct LayerTexture {
    /// The view keeps its texture alive.
    view: wgpu::TextureView,
    size: (u32, u32),
}

impl LayerTexture {
    /// Pixel size.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        self.size
    }
}

/// Affine map from TARGET pixel coordinates to LAYER pixel coordinates
/// (pixel centers at +0.5 on both sides): `q = m * p + t`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine {
    /// Row-major 2x2 part: `[m00, m01, m10, m11]`.
    pub m: [f32; 4],
    /// Translation.
    pub t: [f32; 2],
}

impl Affine {
    /// Pure translation: the layer's top-left sits at `(x, y)` in the
    /// target.
    #[must_use]
    pub fn placed_at(x: f32, y: f32) -> Self {
        Self {
            m: [1.0, 0.0, 0.0, 1.0],
            t: [-x, -y],
        }
    }
}

/// One layer of a frame.
pub struct DrawLayer<'a> {
    /// Pixels.
    pub texture: &'a LayerTexture,
    /// Target → layer mapping.
    pub to_layer: Affine,
    /// 0..=1.
    pub opacity: f32,
    /// Blend mode id (0 normal, 1 multiply, 2 screen, 3 overlay,
    /// 4 darken, 5 lighten — same ids as the CPU reference).
    pub blend: u32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Params {
    m: [f32; 4],
    t_size: [f32; 4],
    misc: [f32; 4],
}

/// Requests a device without any window (workers without UI, tests).
/// `None` when no adapter is available (CI without GPU).
#[must_use]
pub fn request_headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    pollster::block_on(async {
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::all();
        let instance = wgpu::Instance::new(desc);
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: None,
                force_fallback_adapter: false,
                apply_limit_buckets: false,
            })
            .await
            .ok()?;
        adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("cygnus-compositor-headless"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .ok()
    })
}

fn compute_pipeline(device: &wgpu::Device, source: &str, label: &str) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

fn mip_count(width: u32, height: u32) -> u32 {
    32 - width.max(height).max(1).leading_zeros()
}

fn view_level(texture: &wgpu::Texture, level: u32) -> wgpu::TextureView {
    texture.create_view(&wgpu::TextureViewDescriptor {
        base_mip_level: level,
        mip_level_count: Some(1),
        ..Default::default()
    })
}

/// Size-dependent targets, rebuilt only when the frame size changes.
struct Targets {
    size: (u32, u32),
    acc: [wgpu::Texture; 2],
    output: wgpu::Texture,
}

/// The compositor: pipelines + reusable targets, bound to one device.
pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    composite: wgpu::ComputePipeline,
    present: wgpu::ComputePipeline,
    downsample: wgpu::ComputePipeline,
    targets: Option<Targets>,
    /// Accumulator holding the last frame (straight alpha).
    last_acc: usize,
}

impl Compositor {
    /// Builds the pipelines on `device` (handles are cheap clones).
    #[must_use]
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            composite: compute_pipeline(
                device,
                include_str!("shaders/composite.wgsl"),
                "composite",
            ),
            present: compute_pipeline(device, include_str!("shaders/present.wgsl"), "present"),
            downsample: compute_pipeline(
                device,
                include_str!("shaders/downsample.wgsl"),
                "downsample",
            ),
            device: device.clone(),
            queue: queue.clone(),
            targets: None,
            last_acc: 0,
        }
    }

    /// Largest frame / layer edge this device accepts.
    #[must_use]
    pub fn max_dimension(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    /// Uploads straight-alpha RGBA8 pixels (`width * height * 4` bytes).
    /// `None` for an empty / oversized image or a size mismatch.
    #[must_use]
    pub fn upload_rgba(&self, width: u32, height: u32, rgba: &[u8]) -> Option<LayerTexture> {
        let max = self.max_dimension();
        if width == 0 || height == 0 || width > max || height > max {
            return None;
        }
        if rgba.len() != width as usize * height as usize * 4 {
            return None;
        }
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("compositor-layer"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        self.queue.write_texture(
            texture.as_image_copy(),
            rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(width * 4),
                rows_per_image: None,
            },
            size,
        );
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        Some(LayerTexture {
            view,
            size: (width, height),
        })
    }

    fn ensure_targets(&mut self, size: (u32, u32)) -> Option<&Targets> {
        if self.targets.as_ref().is_none_or(|t| t.size != size) {
            let extent = wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            };
            let acc = || {
                self.device.create_texture(&wgpu::TextureDescriptor {
                    label: Some("compositor-acc"),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: FORMAT,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING
                        | wgpu::TextureUsages::STORAGE_BINDING
                        | wgpu::TextureUsages::RENDER_ATTACHMENT
                        | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &[],
                })
            };
            let output = self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("compositor-output"),
                size: extent,
                mip_level_count: mip_count(size.0, size.1),
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: FORMAT,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::STORAGE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            self.targets = Some(Targets {
                size,
                acc: [acc(), acc()],
                output,
            });
        }
        self.targets.as_ref()
    }

    /// Composites `layers` (bottom first) into a `size` frame and returns
    /// the output texture (premultiplied, gamma space, full mip chain),
    /// stable across frames of the same size. `None` for an empty or
    /// oversized frame.
    pub fn composite(
        &mut self,
        size: (u32, u32),
        layers: &[DrawLayer<'_>],
    ) -> Option<&wgpu::Texture> {
        let max = self.max_dimension();
        if size.0 == 0 || size.1 == 0 || size.0 > max || size.1 > max {
            return None;
        }
        let device = self.device.clone();
        let composite_layout = self.composite.get_bind_group_layout(0);
        let present_layout = self.present.get_bind_group_layout(0);
        let downsample_layout = self.downsample.get_bind_group_layout(0);
        let targets = self.ensure_targets(size)?;
        let acc_views = [
            targets.acc[0].create_view(&wgpu::TextureViewDescriptor::default()),
            targets.acc[1].create_view(&wgpu::TextureViewDescriptor::default()),
        ];
        let groups = (size.0.div_ceil(WORKGROUP), size.1.div_ceil(WORKGROUP));
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("compositor-frame"),
        });
        // 1. Transparent start (a clearing render pass: `clear_texture`
        // needs an optional feature the UI device may lack).
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("compositor-clear"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &acc_views[0],
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
            multiview_mask: None,
        }));
        let mut current = 0usize;
        // 2. One pass per layer.
        let mut keep_alive = Vec::with_capacity(layers.len());
        for layer in layers {
            let (lw, lh) = layer.texture.size;
            let params = Params {
                m: layer.to_layer.m,
                t_size: [
                    layer.to_layer.t[0],
                    layer.to_layer.t[1],
                    lw as f32,
                    lh as f32,
                ],
                misc: [
                    layer.opacity.clamp(0.0, 1.0),
                    layer.blend as f32,
                    size.0 as f32,
                    size.1 as f32,
                ],
            };
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("compositor-params"),
                contents: bytemuck::bytes_of(&params),
                usage: wgpu::BufferUsages::UNIFORM,
            });
            let next = 1 - current;
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("compositor-layer-bind"),
                layout: &composite_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&acc_views[current]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&layer.texture.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&acc_views[next]),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: uniform.as_entire_binding(),
                    },
                ],
            });
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                    label: Some("compositor-layer"),
                    timestamp_writes: None,
                });
                pass.set_pipeline(&self.composite);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(groups.0, groups.1, 1);
            }
            keep_alive.push(uniform);
            current = next;
        }
        // 3. Present (premultiply) + mip chain.
        let targets = self.targets.as_ref()?;
        let levels = targets.output.mip_level_count();
        let level0 = view_level(&targets.output, 0);
        let present_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compositor-present-bind"),
            layout: &present_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&acc_views[current]),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&level0),
                },
            ],
        });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compositor-present"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.present);
            pass.set_bind_group(0, &present_bind, &[]);
            pass.dispatch_workgroups(groups.0, groups.1, 1);
        }
        for level in 1..levels {
            let src = view_level(&targets.output, level - 1);
            let dst = view_level(&targets.output, level);
            let w = (size.0 >> level).max(1);
            let h = (size.1 >> level).max(1);
            let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("compositor-mip-bind"),
                layout: &downsample_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&src),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&dst),
                    },
                ],
            });
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("compositor-mip"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.downsample);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(w.div_ceil(WORKGROUP), h.div_ceil(WORKGROUP), 1);
        }
        self.queue.submit(Some(encoder.finish()));
        drop(keep_alive);
        self.last_acc = current;
        self.targets.as_ref().map(|t| &t.output)
    }

    /// Output texture of the last frame, if any.
    #[must_use]
    pub fn output(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().map(|t| &t.output)
    }

    /// Reads the last frame back as STRAIGHT-alpha RGBA8 rows (blocking:
    /// tests, CPU consumers like the histogram). `None` before any frame.
    #[must_use]
    pub fn read_rgba(&self) -> Option<Vec<u8>> {
        let targets = self.targets.as_ref()?;
        read_texture(&self.device, &self.queue, &targets.acc[self.last_acc], 0)
    }

    /// Reads mip `level` of the output (premultiplied), blocking.
    #[must_use]
    pub fn read_output_level(&self, level: u32) -> Option<Vec<u8>> {
        let targets = self.targets.as_ref()?;
        read_texture(&self.device, &self.queue, &targets.output, level)
    }
}

/// Blocking copy of one texture level into CPU memory (tight rows).
fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
    level: u32,
) -> Option<Vec<u8>> {
    let w = (texture.width() >> level).max(1);
    let h = (texture.height() >> level).max(1);
    let pitch =
        (w * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("compositor-readback"),
        size: u64::from(pitch) * u64::from(h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("compositor-readback"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: level,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pitch),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    buffer.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .ok()?;
    rx.recv().ok()?.ok()?;
    let mapped = buffer.slice(..).get_mapped_range().ok()?;
    let mut out = Vec::with_capacity(w as usize * h as usize * 4);
    for row in mapped.chunks_exact(pitch as usize) {
        out.extend_from_slice(&row[..w as usize * 4]);
    }
    drop(mapped);
    buffer.unmap();
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu() -> Option<Compositor> {
        let Some((device, queue)) = request_headless_device() else {
            eprintln!("no GPU adapter: test skipped");
            return None;
        };
        Some(Compositor::new(&device, &queue))
    }

    fn solid(w: u32, h: u32, px: [u8; 4]) -> Vec<u8> {
        px.iter()
            .copied()
            .cycle()
            .take(w as usize * h as usize * 4)
            .collect()
    }

    #[test]
    fn mip_count_matches_the_largest_edge() {
        assert_eq!(mip_count(1, 1), 1);
        assert_eq!(mip_count(8, 3), 4);
        assert_eq!(mip_count(4000, 3000), 12);
    }

    #[test]
    fn normal_over_and_placement() {
        let Some(mut c) = gpu() else { return };
        let base = c
            .upload_rgba(4, 4, &solid(4, 4, [200, 40, 40, 255]))
            .expect("base");
        let top = c
            .upload_rgba(2, 2, &solid(2, 2, [40, 200, 40, 128]))
            .expect("top");
        let layers = [
            DrawLayer {
                texture: &base,
                to_layer: Affine::placed_at(0.0, 0.0),
                opacity: 1.0,
                blend: 0,
            },
            DrawLayer {
                texture: &top,
                to_layer: Affine::placed_at(1.0, 1.0),
                opacity: 1.0,
                blend: 0,
            },
        ];
        assert!(c.composite((4, 4), &layers).is_some());
        let px = c.read_rgba().expect("readback");
        let at = |x: usize, y: usize| &px[(y * 4 + x) * 4..(y * 4 + x) * 4 + 4];
        assert_eq!(at(0, 0), &[200, 40, 40, 255], "outside the top layer");
        // 40*a + 200*(1-a), a = 128/255 → 119 (truncated)
        let mid = at(1, 1);
        assert!((i16::from(mid[0]) - 119).abs() <= 1, "{mid:?}");
        assert_eq!(mid[3], 255);
        assert_eq!(at(3, 3), &[200, 40, 40, 255]);
    }

    #[test]
    fn multiply_and_opacity() {
        let Some(mut c) = gpu() else { return };
        let base = c.upload_rgba(1, 1, &[200, 100, 50, 255]).expect("base");
        let top = c.upload_rgba(1, 1, &[128, 128, 128, 255]).expect("top");
        let draw = |opacity| {
            [
                DrawLayer {
                    texture: &base,
                    to_layer: Affine::placed_at(0.0, 0.0),
                    opacity: 1.0,
                    blend: 0,
                },
                DrawLayer {
                    texture: &top,
                    to_layer: Affine::placed_at(0.0, 0.0),
                    opacity,
                    blend: 1,
                },
            ]
        };
        c.composite((1, 1), &draw(1.0)).expect("frame");
        let px = c.read_rgba().expect("readback");
        assert!((i16::from(px[0]) - 100).abs() <= 1, "{px:?}");
        c.composite((1, 1), &draw(0.0)).expect("frame");
        assert_eq!(c.read_rgba().expect("readback"), vec![200, 100, 50, 255]);
    }

    #[test]
    fn output_is_premultiplied_with_mips() {
        let Some(mut c) = gpu() else { return };
        let half = c
            .upload_rgba(4, 4, &solid(4, 4, [200, 100, 0, 128]))
            .expect("layer");
        let layers = [DrawLayer {
            texture: &half,
            to_layer: Affine::placed_at(0.0, 0.0),
            opacity: 1.0,
            blend: 0,
        }];
        let out = c.composite((4, 4), &layers).expect("frame");
        assert_eq!(out.mip_level_count(), 3);
        let level0 = c.read_output_level(0).expect("level 0");
        // 200 * 128/255 ≈ 100
        assert!(
            (i16::from(level0[0]) - 100).abs() <= 1,
            "{:?}",
            &level0[..4]
        );
        assert_eq!(level0[3], 128);
        let level2 = c.read_output_level(2).expect("level 2");
        assert_eq!(level2.len(), 4);
        assert!((i16::from(level2[0]) - 100).abs() <= 1);
    }

    #[test]
    fn rejects_empty_and_mismatched_uploads() {
        let Some(mut c) = gpu() else { return };
        assert!(c.upload_rgba(0, 4, &[]).is_none());
        assert!(c.upload_rgba(2, 2, &[0; 3]).is_none());
        assert!(c.composite((0, 4), &[]).is_none());
        // Empty layer list: transparent frame.
        c.composite((2, 1), &[]).expect("frame");
        assert_eq!(c.read_rgba().expect("readback"), vec![0; 8]);
    }
}

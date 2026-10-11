// Cygnus — Suite créative professionnelle open source
// TODO workstream 5: externalize inline WGSL shaders to `shaders/*.wgsl` via
// include_str! (to merge with the future GPU v2 of masks).
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

//! GPU via WGPU 30 — compute shaders for nodes + detection
//! UI rendering already WGPU via iced_wgpu; this module adds GPU PROCESSING

use image::DynamicImage;
use std::mem::{align_of, size_of};
use std::sync::{Arc, OnceLock};
use wgpu::util::DeviceExt;

// ---------------------------------------------------------------------------
// Shared GPU context (integrated into the main canvas via shared wgpu Instance)
// ---------------------------------------------------------------------------

pub struct GpuContext {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    pub adapter_info: String,
    pipeline_bc: wgpu::ComputePipeline,
    pipeline_sat: wgpu::ComputePipeline,
    pipeline_blur: wgpu::ComputePipeline,
    pipeline_blend: wgpu::ComputePipeline,
    pipeline_bc_tex: wgpu::ComputePipeline,
    uniforms_bc_tex: wgpu::Buffer,
}

static GPU: OnceLock<Option<Arc<GpuContext>>> = OnceLock::new();

impl GpuContext {
    pub fn is_available() -> bool {
        Self::get().is_some()
    }
    pub fn get() -> Option<Arc<Self>> {
        GPU.get_or_init(Self::try_new).clone()
    }
    fn try_new() -> Option<Arc<Self>> {
        pollster::block_on(Self::try_new_async()).map(Arc::new)
    }
    async fn try_new_async() -> Option<Self> {
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
        let info = adapter.get_info();
        let adapter_info = format!(
            "{} - {} - {:?} - backend {:?}",
            info.name, info.vendor, info.device_type, info.backend
        );
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Cygnus GPU"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
            })
            .await
            .ok()?;

        let pipeline_bc = Self::create_pipeline(&device, SHADER_BC, "bc");
        let pipeline_sat = Self::create_pipeline(&device, SHADER_SAT, "sat");
        let pipeline_blur = Self::create_pipeline(&device, SHADER_BLUR, "blur");
        let pipeline_blend = Self::create_pipeline(&device, SHADER_BLEND, "blend");

        // Texture -> texture path (zero readback): persistent module + pipeline + uniform
        let module_bc_tex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bc_tex"),
            source: wgpu::ShaderSource::Wgsl(SHADER_BC_TEX.into()),
        });
        let pipeline_bc_tex = create_pipeline(&device, &module_bc_tex);
        let uniforms_bc_tex = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bc_tex_uniforms"),
            size: size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Some(Self {
            device,
            queue,
            adapter_info,
            pipeline_bc,
            pipeline_sat,
            pipeline_blur,
            pipeline_blend,
            pipeline_bc_tex,
            uniforms_bc_tex,
        })
    }
    fn create_pipeline(device: &wgpu::Device, shader: &str, label: &str) -> wgpu::ComputePipeline {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some(label),
            source: wgpu::ShaderSource::Wgsl(shader.into()),
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
    pub fn adapter_info() -> String {
        Self::get()
            .map(|g| g.adapter_info.clone())
            .unwrap_or_else(|| "Aucun GPU détecté - fallback CPU".into())
    }
}

// ---------------------------------------------------------------------------
// Detection for UI — BLOCKING on first call (wgpu init +
// shader compilation via `pollster::block_on`). Call ONLY
// in app-side `tokio::task::spawn_blocking`, never directly in
// a `Task::perform` or in `update()`. Later calls (initialized
// OnceLock) are cheap.
// ---------------------------------------------------------------------------

pub fn detect_gpu_info_sync() -> String {
    if let Some(gpu) = GpuContext::get() {
        let mut s = String::new();
        s.push_str(&format!("GPU: {}\n", gpu.adapter_info));
        s.push_str(&format!(
            "Backend: wgpu 30.0.0 - Device limits: {:?}\n",
            gpu.device.limits()
        ));
        s.push_str("Backends: Vulkan/Metal/DX12/WGL (auto)\n");
        s.push_str("Traitement nodal: GPU compute (branché) + fallback CPU rayon\n");
        s.push_str("Rendu canvas: WGPU via iced_wgpu (textures + shaders)\n");
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        s.push_str(&format!("CPU cores: {} (rayon global pool)\n", cores));
        // Force the rayon pool to use all cores
        s.push_str(&format!(
            "Rayon threads: {} (actifs)\n",
            rayon::current_num_threads()
        ));
        s
    } else {
        let mut s = String::new();
        s.push_str("GPU: non disponible - CPU seul\n");
        let cores = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4);
        s.push_str(&format!("CPU cores: {} (rayon)\n", cores));
        s.push_str(&format!(
            "Rayon threads: {}\n",
            rayon::current_num_threads()
        ));
        s
    }
}
/// Async variant kept for compat — the body stays blocking: prefer
/// `spawn_blocking(detect_gpu_info_sync)` in any `Task::perform`.
pub async fn detect_gpu_info() -> String {
    detect_gpu_info_sync()
}

// ---------------------------------------------------------------------------
// GPU helpers
// ---------------------------------------------------------------------------

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsBc {
    width: u32,
    height: u32,
    brightness: f32,
    contrast: f32,
}
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsSat {
    width: u32,
    height: u32,
    sat: f32,
    _pad: f32,
}
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsMix {
    width: u32,
    height: u32,
    factor: f32,
    _pad: f32,
}
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsBlur {
    width: u32,
    height: u32,
    radius: i32,
    _pad: u32,
}

fn image_to_floats(img: &DynamicImage) -> (Vec<f32>, u32, u32) {
    let rgba = img.to_rgba8();
    let (w, h) = (rgba.width(), rgba.height());
    let raw = rgba.into_raw();
    let mut floats = Vec::with_capacity((w * h * 4) as usize);
    for b in raw {
        floats.push(b as f32 / 255.0);
    }
    (floats, w, h)
}
fn floats_to_image(floats: &[f32], w: u32, h: u32) -> DynamicImage {
    let mut raw = Vec::with_capacity((w * h * 4) as usize);
    for &f in floats {
        raw.push((f.clamp(0.0, 1.0) * 255.0) as u8);
    }
    // BUG: panic only if caller provides inconsistent w/h vs floats length — indicates logic error upstream
    let buf = image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(w, h, raw)
        .expect("floats_to_image: dimensions invalides");
    DynamicImage::ImageRgba8(buf)
}

fn run_compute(
    gpu: &GpuContext,
    pipeline: &wgpu::ComputePipeline,
    src_floats: &[f32],
    w: u32,
    h: u32,
    params_bytes: &[u8],
) -> Option<Vec<f32>> {
    let count = (w * h) as usize;
    let src_size = (count * 4 * 4) as u64; // vec4<f32> per pixel
    // Guard rail: respect BOTH wgpu limits
    // - max_buffer_size (buffer creation, 256 MB by default)
    // - max_buffer_binding_size (shader binding, 128 MB by default)
    let limits = gpu.device.limits();
    if src_size > limits.max_storage_buffer_binding_size || src_size > limits.max_buffer_size {
        return None; // -> CPU fallback (or bands via run_compute_banded)
    }
    // src buffer
    let src_buf = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("src"),
            contents: bytemuck::cast_slice(src_floats),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
    let dst_buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dst"),
        size: src_size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: src_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let params_buf = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: params_bytes,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: src_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: dst_buf.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params_buf.as_entire_binding(),
            },
        ],
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("encoder"),
        });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("compute"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        let (wx, wy) = (w.div_ceil(8), h.div_ceil(8));
        pass.dispatch_workgroups(wx, wy, 1);
    }
    encoder.copy_buffer_to_buffer(&dst_buf, 0, &readback, 0, src_size);
    gpu.queue.submit(Some(encoder.finish()));
    // Map
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = gpu
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .ok();
    let Ok(map_res) = rx.recv() else {
        return None;
    };
    if map_res.is_err() {
        return None;
    }
    let Ok(mapped) = readback.slice(..).get_mapped_range() else {
        return None;
    };
    let data = mapped.to_vec();
    drop(mapped);
    readback.unmap();
    let floats: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&data).to_vec();
    // make sure the size matches
    if floats.len() != src_floats.len() {
        return None;
    }
    Some(floats)
}

// ---------------------------------------------------------------------------
// Public API — tries GPU, returns None when unavailable -> CPU fallback
// ---------------------------------------------------------------------------

/// Per-row-band budget: stays under max_storage_buffer_binding_size while
/// keeping good performance.
const BAND_BUDGET_BYTES: u64 = 64 * 1024 * 1024;

/// Runs a compute in row BANDS: each buffer/binding stays under wgpu
/// limits even on very large images (exact point-wise ops).
fn run_compute_banded(
    gpu: &GpuContext,
    pipeline: &wgpu::ComputePipeline,
    src_floats: &[f32],
    w: u32,
    h: u32,
    make_params: impl Fn(u32, u32) -> Vec<u8>,
) -> Option<Vec<f32>> {
    let row_bytes = (w as u64) * 16; // vec4<f32> per pixel
    if row_bytes == 0 {
        return None;
    }
    let limits = gpu.device.limits();
    let budget = limits
        .max_storage_buffer_binding_size
        .min(limits.max_buffer_size)
        .min(BAND_BUDGET_BYTES);
    let rows_per_band = (budget / row_bytes).max(1) as usize;
    let floats_per_row = (w as usize) * 4;

    let mut out = vec![0f32; src_floats.len()];
    for start in (0..h as usize).step_by(rows_per_band) {
        let rows = rows_per_band.min(h as usize - start);
        let lo = start * floats_per_row;
        let hi = lo + rows * floats_per_row;
        let band_out = run_compute(
            gpu,
            pipeline,
            &src_floats[lo..hi],
            w,
            rows as u32,
            &make_params(w, rows as u32),
        )?;
        out[lo..hi].copy_from_slice(&band_out);
    }
    Some(out)
}

pub fn apply_brightness_contrast_gpu(
    img: &DynamicImage,
    brightness: f32,
    contrast: f32,
) -> Option<DynamicImage> {
    let gpu = GpuContext::get()?;
    // Optimization: small images < 256x256 stay CPU (GPU overhead)
    if img.width() * img.height() < 65536 {
        return None;
    }
    let (floats, w, h) = image_to_floats(img);
    // Converts CPU brightness/contrast -> GPU
    // CPU: b = brightness*2.55, contrast_factor = 1+contrast/100 or 1+contrast/50
    let contrast_factor = if contrast < 0.0 {
        1.0 + contrast / 100.0
    } else {
        1.0 + contrast / 50.0
    };
    let brightness_norm = brightness * 0.01; // 2.55/255 =0.01
    // Row bands: supports very large resolutions without exceeding wgpu limits
    let out = run_compute_banded(&gpu, &gpu.pipeline_bc, &floats, w, h, |bw, bh| {
        bytemuck::bytes_of(&ParamsBc {
            width: bw,
            height: bh,
            brightness: brightness_norm,
            contrast: contrast_factor,
        })
        .to_vec()
    })?;
    Some(floats_to_image(&out, w, h))
}

pub fn apply_saturation_gpu(img: &DynamicImage, sat: f32) -> Option<DynamicImage> {
    let gpu = GpuContext::get()?;
    if img.width() * img.height() < 65536 {
        return None;
    }
    let (floats, w, h) = image_to_floats(img);
    let sat_clamped = sat.clamp(0.0, 3.0);
    let out = run_compute_banded(&gpu, &gpu.pipeline_sat, &floats, w, h, |bw, bh| {
        bytemuck::bytes_of(&ParamsSat {
            width: bw,
            height: bh,
            sat: sat_clamped,
            _pad: 0.0,
        })
        .to_vec()
    })?;
    Some(floats_to_image(&out, w, h))
}

pub fn apply_mix_gpu(a: &DynamicImage, b: &DynamicImage, factor: f32) -> Option<DynamicImage> {
    // 2-input mix: no dedicated shader (the Layer node covers this via SHADER_BLEND)
    // → direct CPU fallback, no useless GPU dispatch.
    let _ = (a, b, factor);
    None
}

pub fn apply_blur_gpu(img: &DynamicImage, radius: f32) -> Option<DynamicImage> {
    let gpu = GpuContext::get()?;
    if radius <= 0.1 {
        return Some(img.clone());
    }
    if img.width() * img.height() < 65536 {
        return None;
    }
    let r = (radius as i32).clamp(1, 50);
    let (floats, w, h) = image_to_floats(img);
    let params = ParamsBlur {
        width: w,
        height: h,
        radius: r,
        _pad: 0,
    };
    let out = run_compute(
        &gpu,
        &gpu.pipeline_blur,
        &floats,
        w,
        h,
        bytemuck::bytes_of(&params),
    )?;
    Some(floats_to_image(&out, w, h))
}

// ---------------------------------------------------------------------------
// Layer blending (GPU) — two inputs, opacity + mode
// ---------------------------------------------------------------------------

/// Runs a TWO-source pipeline in row bands.
/// Layout bindings : 0=srcA(read) 1=srcB(read) 2=dst(rw) 3=params(uniform)
fn run_compute2(
    gpu: &GpuContext,
    pipeline: &wgpu::ComputePipeline,
    src_a: &[f32],
    src_b: &[f32],
    w: u32,
    h: u32,
    params_bytes: &[u8],
) -> Option<Vec<f32>> {
    use wgpu::util::DeviceExt;
    let count = (w * h) as usize;
    let size = (count * 4 * 4) as u64;
    let limits = gpu.device.limits();
    if size > limits.max_storage_buffer_binding_size || size > limits.max_buffer_size {
        return None; // -> fallback CPU
    }

    let buf_a = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("srcA"),
            contents: bytemuck::cast_slice(src_a),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
    let buf_b = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("srcB"),
            contents: bytemuck::cast_slice(src_b),
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });
    let dst = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("dst"),
        size,
        usage: wgpu::BufferUsages::STORAGE
            | wgpu::BufferUsages::COPY_SRC
            | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let params_buf = gpu
        .device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("params"),
            contents: params_bytes,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
    let bind_group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bg2"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: buf_a.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: buf_b.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: dst.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: params_buf.as_entire_binding(),
            },
        ],
    });
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("enc2"),
        });
    {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
            label: Some("compute2"),
            timestamp_writes: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.dispatch_workgroups(w.div_ceil(8), h.div_ceil(8), 1);
    }
    encoder.copy_buffer_to_buffer(&dst, 0, &readback, 0, size);
    gpu.queue.submit(Some(encoder.finish()));
    let (tx, rx) = std::sync::mpsc::channel();
    readback.slice(..).map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    let _ = gpu
        .device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: None,
        })
        .ok();
    if rx.recv().ok()?.is_err() {
        return None;
    }
    let data = readback.slice(..).get_mapped_range().ok()?.to_vec();
    readback.unmap();
    let floats: Vec<f32> = bytemuck::cast_slice::<u8, f32>(&data).to_vec();
    if floats.len() != src_a.len() {
        return None;
    }
    Some(floats)
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
struct ParamsBlend2 {
    width: u32,
    height: u32,
    top_w: u32,
    top_h: u32,
    opacity: f32,
    mode: u32,
    offset_x: f32,
    offset_y: f32,
}

const BLEND_BUDGET_BYTES: u64 = 64 * 1024 * 1024;

pub fn apply_blend_gpu(
    base: &DynamicImage,
    top: &DynamicImage,
    opacity_pct: f32,
    mode: u32,
    offset_x: f32,
    offset_y: f32,
) -> Option<DynamicImage> {
    // Non-zero offset: CPU fallback (complex banded offset shader)
    if offset_x != 0.0 || offset_y != 0.0 {
        return None;
    }
    let gpu = GpuContext::get()?;
    if base.width() * base.height() < 65536 {
        return None;
    }
    let (a_floats, w, h) = image_to_floats(base);
    let (mut b_floats, w2, h2) = image_to_floats(top);
    if w != w2 || h != h2 {
        // resize_exact: `resize` preserves the ratio and would give a
        // different size than w×h → out-of-bounds slices below (crash).
        let t_img = floats_to_image(&b_floats, w2, h2).resize_exact(
            w,
            h,
            ::image::imageops::FilterType::Triangle,
        );
        b_floats = image_to_floats(&t_img).0;
    }
    // Garde-fou : tailles strictement identiques, sinon fallback CPU
    if b_floats.len() != a_floats.len() {
        return None;
    }
    let op = (opacity_pct / 100.0).clamp(0.0, 1.0);

    let row_bytes = (w as u64) * 16;
    if row_bytes == 0 {
        return None;
    }
    let budget = limits_min(&gpu).min(BLEND_BUDGET_BYTES);
    let rows_per_band = (budget / row_bytes).max(1) as usize;
    let floats_per_row = (w as usize) * 4;
    let mut out = vec![0f32; a_floats.len()];
    for start in (0..h as usize).step_by(rows_per_band) {
        let rows = rows_per_band.min(h as usize - start);
        let lo = start * floats_per_row;
        let hi = lo + rows * floats_per_row;
        let params = ParamsBlend2 {
            width: w,
            height: rows as u32,
            top_w: w,
            top_h: h,
            opacity: op,
            mode,
            offset_x: 0.0,
            offset_y: 0.0,
        };
        let band_out = run_compute2(
            &gpu,
            &gpu.pipeline_blend,
            &a_floats[lo..hi],
            &b_floats[lo..hi],
            w,
            rows as u32,
            bytemuck::bytes_of(&params),
        )?;
        out[lo..hi].copy_from_slice(&band_out);
    }
    Some(floats_to_image(&out, w, h))
}

fn limits_min(gpu: &GpuContext) -> u64 {
    let l = gpu.device.limits();
    l.max_storage_buffer_binding_size.min(l.max_buffer_size)
}

// ---------------------------------------------------------------------------
// WGSL shaders
// ---------------------------------------------------------------------------

const SHADER_BC: &str = include_str!("shaders/shader_bc.wgsl");

const SHADER_SAT: &str = include_str!("shaders/shader_sat.wgsl");

const SHADER_BLUR: &str = include_str!("shaders/shader_blur.wgsl");

const SHADER_BLEND: &str = include_str!("shaders/shader_blend.wgsl");

/// Brightness/contrast over the TEXTURE -> STORAGE TEXTURE path: pixels
/// never leave VRAM (no CPU readback). Dimensions are read directly
/// from the input texture (no possible desync).
/// NB: no multiple-swizzle assignment (`c.rgb = …`) — unsupported by
/// naga; go through an intermediate variable.
const SHADER_BC_TEX: &str = include_str!("shaders/shader_bc_tex.wgsl");

// ---------------------------------------------------------------------------
// Texture -> texture filter (zero readback)
//
// Unlike the storage-buffer path above (CPU <-> GPU round trip), this
// pipeline reads one texture and writes to another: pixels NEVER leave
// VRAM. Compatible with the "state-only" model: changing a setting comes
// down to queue.write_buffer + redispatch, with no pixel re-upload.
// ---------------------------------------------------------------------------

/// Common format for the filter's input/output textures.
pub const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Filter workgroup size: 16x16 pixels per workgroup.
pub const WORKGROUP_SIZE: u32 = 16;

/// Filter parameters, sent as is via a uniform buffer.
///
/// Application ranges: `brightness` in [-1.0 ; 1.0], `contrast` in [0.0 ; 4.0].
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Uniforms {
    pub brightness: f32,
    pub contrast: f32,
}

impl Uniforms {
    /// Creates bounded parameters: brightness [-1.0 ; 1.0], contrast [0.0 ; 4.0].
    #[must_use]
    pub fn new(brightness: f32, contrast: f32) -> Self {
        Self {
            brightness: brightness.clamp(-1.0, 1.0),
            contrast: contrast.clamp(0.0, 4.0),
        }
    }
}

impl Default for Uniforms {
    /// Neutral setting: no transformation.
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
        }
    }
}

// Compile-time locks: the memory layout must mirror the WGSL struct byte-for-byte
const _: () = assert!(size_of::<Uniforms>() == 8);
const _: () = assert!(align_of::<Uniforms>() == 4);

/// Number of workgroups needed to cover `width x height` pixels.
#[must_use]
pub fn workgroup_count(width: u32, height: u32) -> (u32, u32) {
    (
        width.div_ceil(WORKGROUP_SIZE),
        height.div_ceil(WORKGROUP_SIZE),
    )
}

/// Builds the compute pipeline from an already compiled module.
#[must_use]
pub fn create_pipeline(
    device: &wgpu::Device,
    shader: &wgpu::ShaderModule,
) -> wgpu::ComputePipeline {
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("bc_tex"),
        layout: None,
        module: shader,
        entry_point: Some("main"),
        compilation_options: wgpu::PipelineCompilationOptions::default(),
        cache: None,
    })
}

/// Assembles the bind group: input texture (read), output texture
/// (storage write), settings uniform buffer.
#[must_use]
pub fn create_bind_group(
    device: &wgpu::Device,
    pipeline: &wgpu::ComputePipeline,
    input_view: &wgpu::TextureView,
    output_view: &wgpu::TextureView,
    uniforms: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bc_tex_bg"),
        layout: &pipeline.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(input_view),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(output_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: uniforms.as_entire_binding(),
            },
        ],
    })
}

/// Records the filter dispatch into `encoder` (no submission here: the
/// caller keeps control over its pass batching).
pub fn dispatch_filter(
    encoder: &mut wgpu::CommandEncoder,
    pipeline: &wgpu::ComputePipeline,
    bind_group: &wgpu::BindGroup,
    width: u32,
    height: u32,
) {
    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
        label: Some("bc_tex_dispatch"),
        timestamp_writes: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, bind_group, &[]);
    let (wx, wy) = workgroup_count(width, height);
    pass.dispatch_workgroups(wx, wy, 1);
}

/// Row pitch once aligned to COPY_BYTES_PER_ROW_ALIGNMENT (256 bytes),
/// a wgpu requirement for write_texture/copy_texture_to_texture.
fn row_pitch(width: u32) -> u32 {
    let raw = width * 4;
    raw.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT
}

impl GpuContext {
    /// Creates an RGBA8 texture ready to receive a CPU upload (sampling + copy usage).
    #[must_use]
    pub fn create_input_texture(&self, width: u32, height: u32) -> wgpu::Texture {
        self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bc_tex_in"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        })
    }

    /// Converts `img` to RGBA8 then loads it into a GPU texture. Single pass
    /// through system RAM; afterwards everything stays in VRAM.
    #[must_use]
    pub fn upload_image(&self, img: &DynamicImage) -> wgpu::Texture {
        let rgba = img.to_rgba8();
        let texture = self.create_input_texture(rgba.width(), rgba.height());
        let _ = self.upload_rgba8(&texture, &rgba);
        texture
    }

    /// Writes RGBA8 pixels into `texture`, handling wgpu-imposed row
    /// padding (256-byte aligned pitch). Returns `false` if `rgba` holds
    ///
    /// anything but exactly width x height x 4 bytes.
    pub fn upload_rgba8(&self, texture: &wgpu::Texture, rgba: &[u8]) -> bool {
        let size = texture.size();
        let expected = size.width as usize * size.height as usize * 4;
        if expected == 0 || rgba.len() != expected {
            return false;
        }
        let pitch = row_pitch(size.width);
        let layout = wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(pitch),
            rows_per_image: None,
        };
        if pitch == size.width * 4 {
            // Already aligned rows: direct copy, zero transformation
            self.queue
                .write_texture(texture.as_image_copy(), rgba, layout, size);
        } else {
            let stride = size.width as usize * 4;
            let mut padded = vec![0u8; pitch as usize * size.height as usize];
            for (dst, src) in padded
                .chunks_exact_mut(pitch as usize)
                .zip(rgba.chunks_exact(stride))
            {
                dst[..stride].copy_from_slice(src);
            }
            self.queue
                .write_texture(texture.as_image_copy(), &padded, layout, size);
        }
        true
    }

    /// Applies brightness/contrast: reads `input`, returns a NEW texture.
    ///
    /// ZERO readback: pixels stay in VRAM, the output is directly
    /// chainable to another filter or a render pass. The setting is pushed
    ///
    /// into the context's persistent uniform buffer; queue op ordering
    /// guarantees each dispatch reads the value
    /// written just before it.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use photo_engine::gpu::{GpuContext, Uniforms};
    ///
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let gpu = GpuContext::get().ok_or("GPU indisponible")?;
    ///     let input = gpu.upload_image(&image::DynamicImage::new_rgba8(1920, 1080));
    ///     let output = gpu
    ///         .apply_brightness_contrast_texture(&input, Uniforms::new(0.1, 1.2))
    ///         .ok_or("dimensions invalides")?;
    ///     // `output` stays in VRAM: ready for another filter or display.
    ///     Ok(())
    /// }
    /// ```
    ///
    /// Returns `None` if dimensions are zero or the context is unavailable.
    pub fn apply_brightness_contrast_texture(
        &self,
        input: &wgpu::Texture,
        uniforms: Uniforms,
    ) -> Option<wgpu::Texture> {
        let size = input.size();
        if size.width == 0 || size.height == 0 {
            return None;
        }
        let output = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("bc_tex_out"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let in_view = input.create_view(&wgpu::TextureViewDescriptor::default());
        let out_view = output.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = create_bind_group(
            &self.device,
            &self.pipeline_bc_tex,
            &in_view,
            &out_view,
            &self.uniforms_bc_tex,
        );
        self.queue
            .write_buffer(&self.uniforms_bc_tex, 0, bytemuck::bytes_of(&uniforms));
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("bc_tex_enc"),
            });
        dispatch_filter(
            &mut encoder,
            &self.pipeline_bc_tex,
            &bind_group,
            size.width,
            size.height,
        );
        self.queue.submit(Some(encoder.finish()));
        Some(output)
    }
}

// For compat with old code calling evaluate_gpu_available
pub fn evaluate_gpu_available() -> bool {
    GpuContext::is_available()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniforms_occupe_huit_octets_comme_en_wgsl() {
        assert_eq!(size_of::<Uniforms>(), 8);
        assert_eq!(align_of::<Uniforms>(), 4);
    }

    #[test]
    fn uniforms_par_defaut_sont_neutres() {
        let u = Uniforms::default();
        assert!((u.brightness - 0.0).abs() < f32::EPSILON);
        assert!((u.contrast - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn uniforms_new_borne_les_plages() {
        let hors_plage = Uniforms::new(-5.0, 42.0);
        assert!((hors_plage.brightness - (-1.0)).abs() < f32::EPSILON);
        assert!((hors_plage.contrast - 4.0).abs() < f32::EPSILON);
        let aux_limites = Uniforms::new(1.0, 0.0);
        assert!((aux_limites.brightness - 1.0).abs() < f32::EPSILON);
        assert!((aux_limites.contrast - 0.0).abs() < f32::EPSILON);
        let valide = Uniforms::new(0.25, 1.5);
        assert!((valide.brightness - 0.25).abs() < f32::EPSILON);
        assert!((valide.contrast - 1.5).abs() < f32::EPSILON);
    }

    #[test]
    fn nb_workgroups_couvre_toute_la_surface() {
        assert_eq!(workgroup_count(1, 1), (1, 1));
        assert_eq!(workgroup_count(WORKGROUP_SIZE, WORKGROUP_SIZE), (1, 1));
        assert_eq!(
            workgroup_count(WORKGROUP_SIZE + 1, WORKGROUP_SIZE + 1),
            (2, 2)
        );
        assert_eq!(workgroup_count(1600, 1200), (100, 75));
        assert_eq!(workgroup_count(0, 0), (0, 0));
    }

    #[test]
    fn pas_de_ligne_aligne_sur_256_octets() {
        // 64 px * 4 = 256 bytes: already a multiple of 256
        assert_eq!(row_pitch(64), 256);
        // 640 px * 4 = 2560 bytes: already a multiple of 256
        assert_eq!(row_pitch(640), 2560);
        // 800 px * 4 = 3200 bytes: rounded up to the next multiple of 256
        assert_eq!(row_pitch(800), 3328);
        assert_eq!(row_pitch(1), 256);
    }
}

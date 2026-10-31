// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! GPU adapter selection, minimal GPU check and device creation.
//!
//! The minimal supported GPU is defined by wgpu features and limits, checked
//! at start-up by [`check_adapter`] (docs/decisions/0014). The device is
//! always requested through [`device_descriptor`], both by the application
//! (through eframe) and without a window, so that the renderer runs with the
//! same requirements everywhere.

use std::fmt;

use crate::format::{COMPUTE_FORMAT, COMPUTE_FORMAT_USAGES};

/// Graphics APIs the renderer runs on: Vulkan, Metal and DirectX 12. OpenGL
/// is excluded, the compositor needs compute shaders and storage textures.
pub const BACKENDS: wgpu::Backends = wgpu::Backends::PRIMARY;

/// Environment variable selecting an adapter by a case-insensitive part of
/// its name, for example `llvmpipe` or `WARP`.
pub const ADAPTER_ENV: &str = "CYGNUS_ADAPTER";

/// Optional wgpu features the renderer requires. None so far.
pub const REQUIRED_FEATURES: wgpu::Features = wgpu::Features::empty();

/// Limits the renderer requires: the WebGPU defaults, which every supported
/// backend guarantees on conforming hardware (8192 px textures, 256 array
/// layers, 128 MiB storage buffers).
pub fn required_limits() -> wgpu::Limits {
    wgpu::Limits::default()
}

/// What an adapter offers, as far as the renderer is concerned.
#[derive(Debug, Clone)]
pub struct Capabilities {
    /// Graphics API of the adapter.
    pub backend: wgpu::Backend,
    /// Optional features of the adapter.
    pub features: wgpu::Features,
    /// Limits of the adapter.
    pub limits: wgpu::Limits,
    /// Usages allowed for the compute format.
    pub compute_format_usages: wgpu::TextureUsages,
}

impl Capabilities {
    /// Reads the capabilities of `adapter`.
    pub fn of(adapter: &wgpu::Adapter) -> Self {
        Self {
            backend: adapter.get_info().backend,
            features: adapter.features(),
            limits: adapter.limits(),
            compute_format_usages: adapter
                .get_texture_format_features(COMPUTE_FORMAT)
                .allowed_usages,
        }
    }
}

/// A requirement an adapter does not meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Missing {
    /// The adapter uses an excluded graphics API (OpenGL, WebGPU...).
    Backend(wgpu::Backend),
    /// Required optional features the adapter lacks.
    Features(wgpu::Features),
    /// A limit below the requirement.
    Limit {
        /// wgpu name of the limit.
        name: &'static str,
        /// Value the renderer requires.
        required: u64,
        /// Value the adapter offers.
        available: u64,
    },
    /// Usages of the compute format the adapter does not allow.
    ComputeFormatUsages(wgpu::TextureUsages),
}

impl fmt::Display for Missing {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Backend(backend) => write!(f, "unsupported graphics API {backend:?}"),
            Self::Features(features) => write!(f, "missing features {features:?}"),
            Self::Limit {
                name,
                required,
                available,
            } => write!(f, "limit {name} is {available}, {required} required"),
            Self::ComputeFormatUsages(usages) => {
                write!(f, "format {COMPUTE_FORMAT:?} lacks usages {usages:?}")
            }
        }
    }
}

/// Lists every requirement `capabilities` do not meet. Empty means the
/// adapter is supported.
pub fn missing(capabilities: &Capabilities) -> Vec<Missing> {
    let mut missing = Vec::new();
    if !BACKENDS.contains(capabilities.backend.into()) {
        missing.push(Missing::Backend(capabilities.backend));
    }
    let features = REQUIRED_FEATURES - capabilities.features;
    if !features.is_empty() {
        missing.push(Missing::Features(features));
    }
    required_limits().check_limits_with_fail_fn(
        &capabilities.limits,
        false,
        |name, required, available| {
            missing.push(Missing::Limit {
                name,
                required,
                available,
            });
        },
    );
    let usages = COMPUTE_FORMAT_USAGES - capabilities.compute_format_usages;
    if !usages.is_empty() {
        missing.push(Missing::ComputeFormatUsages(usages));
    }
    missing
}

/// An adapter that does not meet the minimal requirements.
#[derive(Debug, Clone)]
pub struct UnsupportedAdapter {
    /// Adapter name and graphics API.
    pub adapter: String,
    /// Every requirement it does not meet.
    pub missing: Vec<Missing>,
}

impl fmt::Display for UnsupportedAdapter {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: ", self.adapter)?;
        for (index, missing) in self.missing.iter().enumerate() {
            if index > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{missing}")?;
        }
        Ok(())
    }
}

fn describe(info: &wgpu::AdapterInfo) -> String {
    format!("{} ({:?})", info.name, info.backend)
}

/// Checks that `adapter` meets the minimal requirements of the renderer.
pub fn check_adapter(adapter: &wgpu::Adapter) -> Result<(), UnsupportedAdapter> {
    let missing = missing(&Capabilities::of(adapter));
    if missing.is_empty() {
        Ok(())
    } else {
        Err(UnsupportedAdapter {
            adapter: describe(&adapter.get_info()),
            missing,
        })
    }
}

/// Device request of the renderer. The application passes it to eframe, the
/// headless renderer uses it directly.
pub fn device_descriptor(_adapter: &wgpu::Adapter) -> wgpu::DeviceDescriptor<'static> {
    wgpu::DeviceDescriptor {
        label: Some("cygnus device"),
        required_features: REQUIRED_FEATURES,
        required_limits: required_limits(),
        ..Default::default()
    }
}

/// Why no GPU device could be opened.
#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    /// No adapter of the supported graphics APIs exists on this system.
    #[error("no GPU adapter found (Vulkan, Metal or DirectX 12)")]
    NoAdapter,
    /// `CYGNUS_ADAPTER` matches none of the adapters.
    #[error("no GPU adapter matches {ADAPTER_ENV}={wanted:?} (available: {available})")]
    NoMatchingAdapter {
        /// Value of the environment variable.
        wanted: String,
        /// Names of the available adapters.
        available: String,
    },
    /// Adapters exist, but none meets the minimal requirements.
    #[error("no GPU adapter meets the requirements of Cygnus: {}", list(.0))]
    Unsupported(Vec<UnsupportedAdapter>),
    /// The device request failed.
    #[error("cannot open the GPU device: {0}")]
    Device(#[from] wgpu::RequestDeviceError),
}

fn list(adapters: &[UnsupportedAdapter]) -> String {
    adapters
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" | ")
}

/// An open GPU device, without a window.
pub struct Gpu {
    /// The adapter the device was opened on.
    pub info: wgpu::AdapterInfo,
    /// The device.
    pub device: wgpu::Device,
    /// Its command queue.
    pub queue: wgpu::Queue,
}

/// Preference order between supported adapters: real GPUs first.
fn rank(device_type: wgpu::DeviceType) -> u8 {
    match device_type {
        wgpu::DeviceType::DiscreteGpu => 0,
        wgpu::DeviceType::IntegratedGpu => 1,
        wgpu::DeviceType::VirtualGpu => 2,
        wgpu::DeviceType::Cpu => 3,
        wgpu::DeviceType::Other => 4,
    }
}

/// Creates the wgpu instance, restricted to [`BACKENDS`]. `WGPU_BACKEND` may
/// narrow the choice further.
pub fn instance() -> wgpu::Instance {
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle_from_env();
    descriptor.backends &= BACKENDS;
    wgpu::Instance::new(descriptor)
}

/// Picks the adapter to render with: the one named by `CYGNUS_ADAPTER` if
/// set, otherwise the best supported one.
pub fn select_adapter(instance: &wgpu::Instance) -> Result<wgpu::Adapter, GpuError> {
    let mut adapters = pollster::block_on(instance.enumerate_adapters(BACKENDS));
    if adapters.is_empty() {
        return Err(GpuError::NoAdapter);
    }
    if let Ok(wanted) = std::env::var(ADAPTER_ENV) {
        let needle = wanted.to_lowercase();
        let available = adapters
            .iter()
            .map(|adapter| describe(&adapter.get_info()))
            .collect::<Vec<_>>()
            .join(", ");
        adapters.retain(|adapter| adapter.get_info().name.to_lowercase().contains(&needle));
        if adapters.is_empty() {
            return Err(GpuError::NoMatchingAdapter { wanted, available });
        }
    }
    let mut rejected = Vec::new();
    let mut supported = Vec::new();
    for adapter in adapters {
        match check_adapter(&adapter) {
            Ok(()) => supported.push(adapter),
            Err(unsupported) => rejected.push(unsupported),
        }
    }
    supported
        .into_iter()
        .min_by_key(|adapter| rank(adapter.get_info().device_type))
        .ok_or(GpuError::Unsupported(rejected))
}

/// Opens a GPU device without a window, for the command line, the tests and
/// the agents.
pub fn open_headless() -> Result<Gpu, GpuError> {
    let instance = instance();
    let adapter = select_adapter(&instance)?;
    let (device, queue) = pollster::block_on(adapter.request_device(&device_descriptor(&adapter)))?;
    // wgpu's default handler panics. Errors are caught by error scopes where
    // they can happen; anything else is reported, never a crash.
    device.on_uncaptured_error(std::sync::Arc::new(|error: wgpu::Error| {
        eprintln!("cygnus: uncaught GPU error: {error}");
    }));
    Ok(Gpu {
        info: adapter.get_info(),
        device,
        queue,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capable() -> Capabilities {
        Capabilities {
            backend: wgpu::Backend::Vulkan,
            features: wgpu::Features::empty(),
            limits: required_limits(),
            compute_format_usages: wgpu::TextureUsages::all(),
        }
    }

    #[test]
    fn adapter_meeting_the_requirements_is_supported() {
        assert!(missing(&capable()).is_empty());
    }

    #[test]
    fn opengl_is_rejected() {
        let capabilities = Capabilities {
            backend: wgpu::Backend::Gl,
            ..capable()
        };
        assert_eq!(
            missing(&capabilities),
            vec![Missing::Backend(wgpu::Backend::Gl)]
        );
    }

    #[test]
    fn every_low_limit_is_reported() {
        let mut capabilities = capable();
        capabilities.limits.max_texture_dimension_2d = 4096;
        capabilities.limits.max_texture_array_layers = 64;
        let missing = missing(&capabilities);
        assert_eq!(missing.len(), 2, "{missing:?}");
        assert!(missing.contains(&Missing::Limit {
            name: "max_texture_dimension_2d",
            required: 8192,
            available: 4096,
        }));
    }

    #[test]
    fn compute_format_without_storage_is_rejected() {
        let capabilities = Capabilities {
            compute_format_usages: wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            ..capable()
        };
        assert_eq!(
            missing(&capabilities),
            vec![Missing::ComputeFormatUsages(
                wgpu::TextureUsages::STORAGE_BINDING
            )]
        );
    }

    #[test]
    fn messages_name_the_missing_requirement() {
        let unsupported = UnsupportedAdapter {
            adapter: "Old GPU (Vulkan)".to_owned(),
            missing: vec![Missing::Limit {
                name: "max_texture_array_layers",
                required: 256,
                available: 64,
            }],
        };
        assert_eq!(
            unsupported.to_string(),
            "Old GPU (Vulkan): limit max_texture_array_layers is 64, 256 required"
        );
    }

    #[test]
    fn real_gpus_are_preferred() {
        assert!(rank(wgpu::DeviceType::DiscreteGpu) < rank(wgpu::DeviceType::Cpu));
        assert!(rank(wgpu::DeviceType::IntegratedGpu) < rank(wgpu::DeviceType::Cpu));
    }
}

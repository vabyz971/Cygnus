// Cygnus — Suite créative professionnelle open source
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

//! Hardware detection: CPU, RAM (best-effort) and real GPU adapters
//! enumerated via wgpu — the same backends used for rendering.

use serde::{Deserialize, Serialize};

/// A detected GPU.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GpuInfo {
    pub name: String,
    pub vendor: String,
    pub driver: String,
    /// wgpu backend ("Vulkan", "Metal", "Dx12"…)
    pub api: String,
    pub is_discrete: bool,
}

/// CPU information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CpuInfo {
    pub name: String,
    pub cores: usize,
}

/// Memory information (best-effort; 0 = unknown).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RamInfo {
    pub total_mb: u64,
}

/// Full hardware report.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HardwareReport {
    pub cpu: Option<CpuInfo>,
    pub ram: RamInfo,
    pub gpus: Vec<GpuInfo>,
}

impl HardwareReport {
    /// Synchronous detection — BLOCKING (`Instance::new` +
    /// `pollster::block_on(enumerate_adapters)`).
    /// Call ONLY inside `tokio::task::spawn_blocking` (dedicated
    /// blocking pool). NEVER call it directly inside a
    /// `Task::perform`: that would run on the shared Tokio executor and
    /// starve decoding / export / compositing.
    #[must_use]
    pub fn detect_sync() -> Self {
        let cpu = Some(CpuInfo {
            name: std::env::consts::ARCH.to_string(),
            cores: std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1),
        });

        let ram = RamInfo {
            total_mb: detect_ram_mb(),
        };

        let mut gpus = Vec::new();
        // Same instance pattern as photo-engine/src/gpu.rs (headless-safe)
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::all();
        let instance = wgpu::Instance::new(desc);

        // enumerate_adapters is async in wgpu 30: one-off blocking via
        // pollster, call expected from a background task (never the UI).
        let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
        for adapter in adapters {
            let info = adapter.get_info();
            gpus.push(GpuInfo {
                name: if info.name.is_empty() {
                    format!("GPU {:?}", info.backend)
                } else {
                    info.name.clone()
                },
                vendor: format!("{:X}", info.vendor),
                driver: info.driver,
                api: format!("{:?}", info.backend),
                is_discrete: info.device_type == wgpu::DeviceType::DiscreteGpu,
            });
        }
        gpus.sort_by(|a, b| b.is_discrete.cmp(&a.is_discrete).then(a.name.cmp(&b.name)));

        Self { cpu, ram, gpus }
    }

    /// Async variant (mission convention) — WARNING: the body stays
    /// blocking (`pollster::block_on`). Prefer `spawn_blocking(detect_sync)`
    /// inside any `Task::perform`; this variant only exists for
    /// contexts already off-executor (tests, CLI).
    pub async fn detect() -> Self {
        Self::detect_sync()
    }
}

/// Total RAM in MB — best-effort read of /proc/meminfo on Linux,
/// `sysinfo` deliberately not vendored (heavy dependency for a
/// single information line). Returns 0 when undetermined.
fn detect_ram_mb() -> u64 {
    #[cfg(target_os = "linux")]
    {
        if let Ok(content) = std::fs::read_to_string("/proc/meminfo")
            && let Some(line) = content.lines().find(|l| l.starts_with("MemTotal:"))
            && let Some(kb) = line.split_whitespace().nth(1)
            && let Ok(kb) = kb.parse::<u64>()
        {
            return kb / 1024;
        }
        0
    }
    #[cfg(not(target_os = "linux"))]
    {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detection_ne_panique_jamais_et_renvoie_le_cpu() {
        // On headless CI machines: zero GPUs possible, CPU always present,
        // and above all NO panic — the hardware report's contract.
        let report = HardwareReport::detect_sync();
        let cpu = report.cpu.expect("cpu info");
        assert!(cpu.cores >= 1);
        assert!(!cpu.name.is_empty());
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn ram_linux_est_lue_de_proc_meminfo() {
        let mb = detect_ram_mb();
        assert!(mb > 0, "/proc/meminfo doit fournir MemTotal");
    }
}

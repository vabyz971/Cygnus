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

//! Fluidity benchmark (docs/decisions/0014): the document, the camera path
//! and the report, shared by the headless example and the in-application
//! run, so that both measure exactly the same scenario.
//!
//! Document: 6000 × 4000 px (24 Mpx), 16 bits, ten fully covered pixel
//! layers, Normal blend, opacities from 30 to 100 %. Camera path: fit
//! (cold, then warm), pan at 100 %, continuous zoom from 5 to 800 %,
//! diagonal pan at 300 %, then an opacity change.

use std::time::Duration;

use cygnus_commands::{BusLimits, CommandBus, Source};
use cygnus_photo_core::commands::{DocumentCreate, LayerCreate, LayerUpdate, PixelsWrite};
use cygnus_photo_core::script::ScriptError;
use cygnus_photo_core::{Depth, Document, NodeId, PhotoCommand, Rect};

use crate::canvas::CanvasStats;

/// Benchmark document width, in pixels.
pub const WIDTH: u32 = 6000;
/// Benchmark document height, in pixels.
pub const HEIGHT: u32 = 4000;
/// Number of pixel layers.
pub const LAYERS: u64 = 10;
/// Rows written per command while building the document.
const STRIP: u32 = 256;
/// Layer whose opacity changes in the last phase.
pub const EDITED_LAYER: NodeId = NodeId(5);

/// Phases of the camera path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// First frame at "fit to screen", empty cache.
    FitCold,
    /// Still frames at "fit to screen", warm cache.
    FitWarm,
    /// Pan across the whole width at 100 %.
    Pan100,
    /// Continuous zoom from 5 % to 800 %, centered.
    Zoom,
    /// Diagonal pan at 300 %.
    Pan300,
    /// Frames after an opacity change of one layer.
    Edit,
}

impl Phase {
    /// Stable name, used in the report.
    pub fn name(self) -> &'static str {
        match self {
            Self::FitCold => "fit_cold",
            Self::FitWarm => "fit_warm",
            Self::Pan100 => "pan_100",
            Self::Zoom => "zoom_5_to_800",
            Self::Pan300 => "pan_300",
            Self::Edit => "edit_opacity",
        }
    }
}

/// One frame of the camera path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraFrame {
    /// Phase the frame belongs to.
    pub phase: Phase,
    /// Screen pixels per document pixel.
    pub scale: f32,
    /// Document point at the center of the viewport.
    pub center: [f32; 2],
    /// Whether the opacity change is applied before this frame.
    pub edit: bool,
}

/// Value `t` (0..1) of the way from `a` to `b`.
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The camera path for a viewport of `viewport` pixels.
pub fn camera_path(viewport: [f32; 2]) -> Vec<CameraFrame> {
    let (width, height) = (WIDTH as f32, HEIGHT as f32);
    let middle = [width / 2.0, height / 2.0];
    let fit = 0.92 * (viewport[0] / width).min(viewport[1] / height);
    let frame = |phase, scale, center| CameraFrame {
        phase,
        scale,
        center,
        edit: false,
    };
    let mut path = vec![frame(Phase::FitCold, fit, middle)];
    path.extend((0..120).map(|_| frame(Phase::FitWarm, fit, middle)));
    // Pan at 100 % from the left edge to the right edge, mid-height.
    let half = viewport[0] / 2.0;
    path.extend((0..240).map(|i| {
        let t = i as f32 / 239.0;
        frame(
            Phase::Pan100,
            1.0,
            [lerp(half, width - half, t), height / 2.0],
        )
    }));
    // Zoom from 5 % to 800 %, geometric, centered.
    path.extend((0..240).map(|i| {
        let t = i as f32 / 239.0;
        frame(Phase::Zoom, 0.05 * (8.0f32 / 0.05).powf(t), middle)
    }));
    // Diagonal pan at 300 %, from the top left quarter to the bottom right.
    path.extend((0..120).map(|i| {
        let t = i as f32 / 119.0;
        frame(
            Phase::Pan300,
            3.0,
            [
                lerp(width * 0.25, width * 0.75, t),
                lerp(height * 0.25, height * 0.75, t),
            ],
        )
    }));
    // Opacity change, then frames until the view is up to date and stable.
    path.extend((0..30).map(|i| CameraFrame {
        edit: i == 0,
        ..frame(Phase::Edit, 3.0, [width * 0.75, height * 0.75])
    }));
    path
}

/// Document position on the target for `frame`, rounded to whole pixels as
/// the application does.
pub fn origin(frame: &CameraFrame, viewport: [f32; 4]) -> [f32; 2] {
    let center = [
        viewport[0] + viewport[2] / 2.0,
        viewport[1] + viewport[3] / 2.0,
    ];
    [
        (center[0] - frame.center[0] * frame.scale).round(),
        (center[1] - frame.center[1] * frame.scale).round(),
    ]
}

/// Deterministic pseudo-random value in 0..1 for a pixel of a layer.
fn noise(layer: u64, x: u32, y: u32) -> f32 {
    let mut h = layer
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(u64::from(x) << 32 | u64::from(y));
    h ^= h >> 33;
    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
    h ^= h >> 33;
    (h & 0xFFFF) as f32 / 65535.0
}

/// Commands writing layer `id` over the document, by strips.
fn layer_commands(id: u64) -> Vec<PhotoCommand> {
    let mut commands = vec![PhotoCommand::LayerCreate(LayerCreate {
        name: format!("layer {id}"),
        parent: None,
        index: None,
    })];
    let phase = id as f32 * 0.7;
    for top in (0..HEIGHT).step_by(STRIP as usize) {
        let rows = STRIP.min(HEIGHT - top);
        let mut samples = Vec::with_capacity((WIDTH * rows * 4) as usize);
        for y in top..top + rows {
            for x in 0..WIDTH {
                let gx = x as f32 / WIDTH as f32;
                let gy = y as f32 / HEIGHT as f32;
                let n = noise(id, x, y) * 0.25;
                let rgb = [
                    0.5 + 0.35 * (gx * 6.0 + phase).sin() + n - 0.125,
                    0.5 + 0.35 * (gy * 5.0 + phase).cos() + n - 0.125,
                    0.5 + 0.35 * ((gx + gy) * 4.0 + phase).sin() + n - 0.125,
                ];
                samples.extend(rgb.map(|c| (c.clamp(0.0, 1.0) * 65535.0) as u16));
                samples.push(u16::MAX);
            }
        }
        commands.push(PhotoCommand::PixelsWrite(PixelsWrite {
            layer: NodeId(id),
            rect: Rect {
                x: 0,
                y: top,
                width: WIDTH,
                height: rows,
            },
            samples,
        }));
    }
    // Opacities from 30 % (top) to 100 % (bottom layer).
    let opacity = 1.0 - 0.7 * (id - 1) as f32 / (LAYERS - 1) as f32;
    commands.push(PhotoCommand::LayerUpdate(LayerUpdate {
        opacity: Some(opacity),
        ..LayerUpdate::new(NodeId(id))
    }));
    commands
}

/// Builds the benchmark document through a command bus that keeps no
/// undo data (the benchmark measures display, not history).
pub fn build_document() -> Result<Document, ScriptError> {
    let mut bus = CommandBus::with_limits(BusLimits {
        retained_bytes: 0,
        ..BusLimits::default()
    });
    let mut document = Document::default();
    let mut run = |command: PhotoCommand| {
        let name = cygnus_commands::Command::name(&command);
        bus.execute(&mut document, command, Source::Test)
            .map(|_| ())
            .map_err(|error| ScriptError::Command {
                index: 0,
                name,
                error,
            })
    };
    run(PhotoCommand::DocumentCreate(DocumentCreate {
        width: WIDTH,
        height: HEIGHT,
        resolution: 300.0,
        depth: Depth::U16,
        profile: Default::default(),
    }))?;
    for id in 1..=LAYERS {
        for command in layer_commands(id) {
            run(command)?;
        }
    }
    Ok(document)
}

/// The command of the last phase.
pub fn edit_command() -> PhotoCommand {
    PhotoCommand::LayerUpdate(LayerUpdate {
        opacity: Some(0.5),
        ..LayerUpdate::new(EDITED_LAYER)
    })
}

/// Memory used by the pixels of `document`, in bytes.
pub fn document_bytes(document: &Document) -> u64 {
    document
        .pixels
        .layers()
        .flat_map(|layer| document.pixels.tiles(layer))
        .map(|(_, tile)| tile.bytes() as u64)
        .sum()
}

/// Measurements of one frame.
#[derive(Debug, Clone, Copy)]
pub struct FrameSample {
    /// Phase of the frame.
    pub phase: Phase,
    /// Frame time.
    pub time: Duration,
    /// Canvas counters after the frame.
    pub stats: CanvasStats,
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len());
    sorted[rank - 1]
}

/// Machine and run description written in the report.
#[derive(Debug, Clone, Default)]
pub struct RunInfo {
    /// "headless" or "application".
    pub mode: String,
    /// How a frame time is measured.
    pub timing: String,
    /// GPU adapter name, API and driver.
    pub adapter: String,
    /// Viewport size in pixels.
    pub viewport: [u32; 2],
    /// GPU memory of the canvas cache, in bytes.
    pub cache_bytes: u64,
    /// CPU memory of the document pixels, in bytes.
    pub document_bytes: u64,
}

fn read_first(path: &str, prefix: &str) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    text.lines()
        .find(|line| line.starts_with(prefix))
        .and_then(|line| line.split_once([':', '=']))
        .map(|(_, value)| value.trim().trim_matches('"').to_owned())
}

/// Best-effort description of the machine (complete on Linux).
pub fn machine() -> serde_json::Value {
    serde_json::json!({
        "os": read_first("/etc/os-release", "PRETTY_NAME")
            .unwrap_or_else(|| std::env::consts::OS.to_owned()),
        "cpu": read_first("/proc/cpuinfo", "model name").unwrap_or_else(|| "unknown".to_owned()),
        "logical_cpus": std::thread::available_parallelism().map_or(0, |n| n.get()),
        "memory": read_first("/proc/meminfo", "MemTotal").unwrap_or_else(|| "unknown".to_owned()),
        "arch": std::env::consts::ARCH,
    })
}

/// The JSON report of a run.
pub fn report(info: &RunInfo, samples: &[FrameSample]) -> serde_json::Value {
    let mut phases = Vec::new();
    let mut previous = CanvasStats::default();
    let mut index = 0;
    while index < samples.len() {
        let phase = samples[index].phase;
        let start = previous;
        let mut times = Vec::new();
        while index < samples.len() && samples[index].phase == phase {
            times.push(samples[index].time.as_secs_f64() * 1000.0);
            previous = samples[index].stats;
            index += 1;
        }
        let mut sorted = times.clone();
        sorted.sort_by(f64::total_cmp);
        let median = percentile(&sorted, 0.5);
        phases.push(serde_json::json!({
            "phase": phase.name(),
            "frames": times.len(),
            "median_ms": median,
            "p95_ms": percentile(&sorted, 0.95),
            "p99_ms": percentile(&sorted, 0.99),
            "max_ms": sorted.last().copied().unwrap_or(0.0),
            "fps_at_median": if median > 0.0 { 1000.0 / median } else { 0.0 },
            "p95_within_16_7_ms": percentile(&sorted, 0.95) <= 1000.0 / 60.0,
            "tiles_composited": previous.composited - start.composited,
            "tiles_downsampled": previous.downsampled - start.downsampled,
            "tiles_evicted": previous.evicted - start.evicted,
        }));
    }
    serde_json::json!({
        "benchmark": "fluidity",
        "decision": "docs/decisions/0014",
        "mode": info.mode,
        "timing": info.timing,
        "document": {
            "width": WIDTH,
            "height": HEIGHT,
            "depth_bits": 16,
            "layers": LAYERS,
            "pixel_memory_bytes": info.document_bytes,
        },
        "viewport": info.viewport,
        "adapter": info.adapter,
        "machine": machine(),
        "canvas_cache_bytes": info.cache_bytes,
        "phases": phases,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn camera_path_covers_every_phase_in_order() {
        let path = camera_path([2560.0, 1440.0]);
        let mut phases: Vec<Phase> = path.iter().map(|frame| frame.phase).collect();
        phases.dedup();
        assert_eq!(
            phases,
            [
                Phase::FitCold,
                Phase::FitWarm,
                Phase::Pan100,
                Phase::Zoom,
                Phase::Pan300,
                Phase::Edit
            ]
        );
        assert_eq!(path.iter().filter(|frame| frame.edit).count(), 1);
        let zoom: Vec<f32> = path
            .iter()
            .filter(|frame| frame.phase == Phase::Zoom)
            .map(|frame| frame.scale)
            .collect();
        assert!((zoom[0] - 0.05).abs() < 1e-6);
        assert!((zoom[zoom.len() - 1] - 8.0).abs() < 1e-3);
    }

    #[test]
    fn percentiles_use_nearest_rank() {
        let sorted: Vec<f64> = (1..=100).map(f64::from).collect();
        assert_eq!(percentile(&sorted, 0.5), 50.0);
        assert_eq!(percentile(&sorted, 0.95), 95.0);
        assert_eq!(percentile(&sorted, 1.0), 100.0);
    }

    #[test]
    fn noise_is_deterministic() {
        assert_eq!(noise(3, 10, 20), noise(3, 10, 20));
        assert_ne!(noise(3, 10, 20), noise(4, 10, 20));
    }
}

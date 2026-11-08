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
//! Document: 6000 × 4000 px (24 Mpx), 16 bits, ten layers (docs/decisions/
//! 0027 and 0028): seven fully covered pixel layers, Normal blend,
//! opacities from 30 to 100 %, and three adjustment layers (curves, levels,
//! hue/saturation); two gradient masks, on a pixel layer and on the levels.
//! Camera path: fit (cold, then warm), pan at 100 %, continuous zoom from
//! 5 to 800 %, diagonal pan at 300 %, an opacity change, then a curve point
//! dragged at 300 % and at fit.
//!
//! Within a frame budget (docs/decisions/0034), the canvas may need several
//! frames to show the final image: a frame marked `settle` is repeated, in
//! the same phase, until the canvas has no work left.

use std::time::Duration;

use cygnus_commands::{BusLimits, CommandBus, Source};
use cygnus_photo_core::adjust::{Adjustment, Curve, Curves, HueSaturation, Levels, LevelsChannel};
use cygnus_photo_core::commands::{
    AdjustmentUpdate, DocumentCreate, LayerCreate, LayerKind, LayerUpdate, MaskCreate,
    MaskGradient, PixelsWrite,
};
use cygnus_photo_core::script::ScriptError;
use cygnus_photo_core::{Depth, Document, NodeId, PhotoCommand, Rect};

use crate::canvas::CanvasStats;
use crate::sources::SourceStats;

/// Benchmark document width, in pixels.
pub const WIDTH: u32 = 6000;
/// Benchmark document height, in pixels.
pub const HEIGHT: u32 = 4000;
/// Number of layers, adjustments included.
pub const LAYERS: u64 = 10;
/// Number of pixel layers.
pub const PIXEL_LAYERS: u64 = 7;
/// Rows written per command while building the document.
const STRIP: u32 = 256;
/// Layer whose opacity changes in the edit phase.
pub const EDITED_LAYER: NodeId = NodeId(6);
/// Curves adjustment whose point is dragged.
pub const CURVES_LAYER: NodeId = NodeId(5);
/// Frames of each curve drag phase.
const DRAG_FRAMES: u32 = 60;

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
    /// A curve point dragged, one change per frame, at 300 %.
    CurveDrag300,
    /// A curve point dragged, one change per frame, at fit.
    CurveDragFit,
    /// The frame after the drag at fit: the preview is refined exactly.
    CurveRelease,
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
            Self::CurveDrag300 => "curve_drag_300",
            Self::CurveDragFit => "curve_drag_fit",
            Self::CurveRelease => "curve_release",
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
    /// Change applied before this frame.
    pub edit: Option<Edit>,
    /// A parameter is being dragged: the canvas previews (0027, B7-lite).
    pub preview: bool,
    /// Repeated, without its edit, until the canvas has no work left.
    pub settle: bool,
}

/// A change of the document during the camera path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Edit {
    /// Opacity of [`EDITED_LAYER`] to 50 %.
    Opacity,
    /// Middle point of the master curve of [`CURVES_LAYER`] at step `n` of
    /// a drag.
    Curve(u32),
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
        edit: None,
        preview: false,
        settle: false,
    };
    let mut path = vec![CameraFrame {
        settle: true,
        ..frame(Phase::FitCold, fit, middle)
    }];
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
        edit: (i == 0).then_some(Edit::Opacity),
        settle: i == 0,
        ..frame(Phase::Edit, 3.0, [width * 0.75, height * 0.75])
    }));
    // A curve point dragged, one change per frame, at 300 % then at fit.
    for (phase, scale, center) in [
        (Phase::CurveDrag300, 3.0, [width * 0.75, height * 0.75]),
        (Phase::CurveDragFit, fit, middle),
    ] {
        path.extend((0..DRAG_FRAMES).map(|i| CameraFrame {
            edit: Some(Edit::Curve(i)),
            preview: true,
            ..frame(phase, scale, center)
        }));
    }
    path.push(CameraFrame {
        settle: true,
        ..frame(Phase::CurveRelease, fit, middle)
    });
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

/// Commands writing layer `id` over the document, by strips: pixel layer
/// `pixel_index` of `pixel_layers`.
fn layer_commands(id: u64, pixel_index: u64, pixel_layers: u64) -> Vec<PhotoCommand> {
    let mut commands = vec![PhotoCommand::LayerCreate(LayerCreate {
        name: format!("layer {id}"),
        parent: None,
        index: None,
        kind: Default::default(),
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
    let opacity = 1.0 - 0.7 * pixel_index as f32 / (pixel_layers.max(2) - 1) as f32;
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
    // Layers 1 to 10, bottom first: pixel layers, with curves (5), levels
    // (8, masked) and hue/saturation (10); layer 9 is masked too.
    let mut pixel_index = 0;
    for id in 1..=LAYERS {
        let adjustment = match id {
            5 => Some(curves(0)),
            8 => Some(Adjustment::Levels(Levels {
                master: LevelsChannel {
                    input_black: 0.04,
                    input_white: 0.93,
                    gamma: 1.15,
                    output_black: 0.0,
                    output_white: 1.0,
                },
                ..Levels::default()
            })),
            10 => Some(Adjustment::HueSaturation(HueSaturation {
                hue: 12.0,
                saturation: 15.0,
                lightness: 0.0,
            })),
            _ => None,
        };
        match adjustment {
            Some(adjustment) => run(PhotoCommand::LayerCreate(LayerCreate {
                name: format!("adjustment {id}"),
                parent: None,
                index: None,
                kind: LayerKind::Adjustment { adjustment },
            }))?,
            None => {
                for command in layer_commands(id, pixel_index, PIXEL_LAYERS) {
                    run(command)?;
                }
                pixel_index += 1;
            }
        }
    }
    // Gradient masks: mask 11 on the levels, mask 12 on pixel layer 9.
    for (node, mask, start, end) in [
        (8, 11, [0.0, 0.0], [WIDTH as f32, 0.0]),
        (9, 12, [0.0, 0.0], [0.0, HEIGHT as f32]),
    ] {
        run(PhotoCommand::MaskCreate(MaskCreate {
            node: NodeId(node),
            fill: Default::default(),
        }))?;
        run(PhotoCommand::MaskGradient(MaskGradient {
            mask: NodeId(mask),
            start,
            end,
            start_value: 0.15,
            end_value: 1.0,
        }))?;
    }
    Ok(document)
}

/// The 8 GiB scenario of docs/decisions/0024: `pixel_layers` (40) fully
/// covered pixel layers of 24 Mpx in 16 bits, without adjustments.
pub fn build_large_document(pixel_layers: u64) -> Result<Document, ScriptError> {
    let mut bus = CommandBus::with_limits(BusLimits {
        retained_bytes: 0,
        ..BusLimits::default()
    });
    let mut document = Document::default();
    let mut commands = vec![PhotoCommand::DocumentCreate(DocumentCreate {
        width: WIDTH,
        height: HEIGHT,
        resolution: 300.0,
        depth: Depth::U16,
        profile: Default::default(),
    })];
    for index in 0..pixel_layers {
        commands.extend(layer_commands(index + 1, index, pixel_layers));
    }
    for command in commands {
        let name = cygnus_commands::Command::name(&command);
        bus.execute(&mut document, command, Source::Test)
            .map_err(|error| ScriptError::Command {
                index: 0,
                name,
                error,
            })?;
    }
    Ok(document)
}

/// Peak resident memory of the process, in bytes (Linux; 0 elsewhere).
pub fn peak_resident_bytes() -> u64 {
    read_first("/proc/self/status", "VmHWM")
        .and_then(|value| value.trim_end_matches("kB").trim().parse::<u64>().ok())
        .map_or(0, |kib| kib * 1024)
}

/// Curves of the curves adjustment at step `step` of the drag: an S curve
/// whose middle point rises.
fn curves(step: u32) -> Adjustment {
    let y = 0.5 + 0.2 * step as f32 / DRAG_FRAMES as f32;
    Adjustment::Curves(Curves {
        master: Curve {
            points: vec![[0.0, 0.0], [0.25, 0.2], [0.5, y], [0.75, 0.82], [1.0, 1.0]],
        },
        ..Curves::default()
    })
}

/// The command of a change of the camera path.
pub fn edit_command(edit: Edit) -> PhotoCommand {
    match edit {
        Edit::Opacity => PhotoCommand::LayerUpdate(LayerUpdate {
            opacity: Some(0.5),
            ..LayerUpdate::new(EDITED_LAYER)
        }),
        Edit::Curve(step) => PhotoCommand::AdjustmentUpdate(AdjustmentUpdate {
            id: CURVES_LAYER,
            adjustment: curves(step),
        }),
    }
}

/// Time to compute the lookup tables of the dragged curve, alone on the
/// CPU: the median and the largest over the steps of a drag, in ms.
pub fn curve_table_time() -> (f64, f64) {
    let mut times: Vec<f64> = (0..DRAG_FRAMES)
        .map(|step| {
            let start = std::time::Instant::now();
            if let Adjustment::Curves(c) = curves(step) {
                for curve in [&c.red, &c.green, &c.blue, &c.master] {
                    std::hint::black_box(curve.lookup_table());
                }
            }
            start.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(f64::total_cmp);
    (
        percentile(&times, 0.5),
        times.last().copied().unwrap_or(0.0),
    )
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
    /// Source pool counters after the frame.
    pub sources: SourceStats,
    /// Every visible tile showed something after the frame.
    pub covered: bool,
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
    /// Slots of the layer and mask source pools.
    pub source_slots: (u32, u32),
    /// Layers and pixel layers of the document, when not the standard
    /// scenario (the 8 GiB one).
    pub layers: Option<(u64, u64)>,
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
    let mut previous_sources = SourceStats::default();
    let mut index = 0;
    while index < samples.len() {
        let phase = samples[index].phase;
        let start = previous;
        let start_sources = previous_sources;
        let mut times = Vec::new();
        let mut first_view: Option<f64> = None;
        while index < samples.len() && samples[index].phase == phase {
            times.push(samples[index].time.as_secs_f64() * 1000.0);
            if first_view.is_none() && samples[index].covered {
                first_view = Some(times.iter().sum());
            }
            previous = samples[index].stats;
            previous_sources = samples[index].sources;
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
            "first_frame_ms": times.first().copied().unwrap_or(0.0),
            "total_ms": times.iter().sum::<f64>(),
            "first_view_ms": first_view,
            "frames_over_100_ms": times.iter().filter(|t| **t > 100.0).count(),
            "fps_at_median": if median > 0.0 { 1000.0 / median } else { 0.0 },
            "p95_within_16_7_ms": percentile(&sorted, 0.95) <= 1000.0 / 60.0,
            "tiles_composited": previous.composited - start.composited,
            "tiles_approximated": previous.approximated - start.approximated,
            "tiles_downsampled": previous.downsampled - start.downsampled,
            "tiles_evicted": previous.evicted - start.evicted,
            "source_tiles_uploaded": previous_sources.uploads - start_sources.uploads,
            "source_tiles_evicted": previous_sources.evictions - start_sources.evictions,
            "curve_tables_uploaded": previous_sources.curve_tables.uploads - start_sources.curve_tables.uploads,
            "curve_table_bytes": previous_sources.curve_tables.bytes - start_sources.curve_tables.bytes,
            "reduced_tiles_built": previous_sources.reduced - start_sources.reduced,
            "reduced_tiles_uploaded": previous_sources.reduced_uploads - start_sources.reduced_uploads,
            "reduced_bytes_held": previous_sources.reduced_bytes,
        }));
    }
    let curve_cpu = curve_table_time();
    serde_json::json!({
        "benchmark": "fluidity",
        "scenario": "docs/decisions/0014",
        "decision": "docs/decisions/0019",
        "mode": info.mode,
        "timing": info.timing,
        "document": {
            "width": WIDTH,
            "height": HEIGHT,
            "depth_bits": 16,
            "layers": info.layers.map_or(LAYERS, |(layers, _)| layers),
            "pixel_layers": info.layers.map_or(PIXEL_LAYERS, |(_, pixels)| pixels),
            "adjustments": ["curves", "levels (masked)", "hue_saturation"],
            "masks": 2,
            "pixel_memory_bytes": info.document_bytes,
        },
        "viewport": info.viewport,
        "adapter": info.adapter,
        "machine": machine(),
        "canvas_cache_bytes": info.cache_bytes,
        "peak_resident_bytes": peak_resident_bytes(),
        "source_pool_slots": info.source_slots,
        "curve_table_cpu_ms": {
            "median": curve_cpu.0,
            "max": curve_cpu.1,
        },
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
                Phase::Edit,
                Phase::CurveDrag300,
                Phase::CurveDragFit,
                Phase::CurveRelease
            ]
        );
        let edits = |wanted: fn(&Edit) -> bool| {
            path.iter()
                .filter(|frame| frame.edit.as_ref().is_some_and(wanted))
                .count()
        };
        assert_eq!(edits(|e| *e == Edit::Opacity), 1);
        assert_eq!(
            edits(|e| matches!(e, Edit::Curve(_))),
            2 * DRAG_FRAMES as usize
        );
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

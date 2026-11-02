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

//! Parameters of adjustment layers for the composite shader
//! (docs/decisions/0028): each adjustment is encoded as 32-bit words in a
//! buffer of the batch, read by `adjust.wgsl`; curves are read from lookup
//! tables kept resident on the GPU, indexed by the content of the curve.

use std::collections::HashMap;
use std::sync::Arc;

use cygnus_photo_core::adjust::{Adjustment, ColorShift, Curve, CurveTable, LevelsChannel};

/// Strength of the color balance: the shift of a channel at ±100 with full
/// weight (docs/decisions/0028). Provisional until judged on the
/// comparison sheets.
pub const COLOR_BALANCE_STRENGTH: f32 = 0.7;

/// Largest contrast multiplier: a threshold at 0.5 for contrast +100.
pub const MAX_CONTRAST_SLOPE: f64 = 131_072.0;

/// Adjustment kinds, as in the composite shader (`ADJUST_*`, generated).
pub(crate) const KINDS: [(&str, u32); 5] = [
    ("ADJUST_LEVELS", 0),
    ("ADJUST_CURVES", 1),
    ("ADJUST_HUE_SATURATION", 2),
    ("ADJUST_BRIGHTNESS_CONTRAST", 3),
    ("ADJUST_COLOR_BALANCE", 4),
];

pub(crate) fn kind(adjustment: &Adjustment) -> u32 {
    match adjustment {
        Adjustment::Levels(_) => 0,
        Adjustment::Curves(_) => 1,
        Adjustment::HueSaturation(_) => 2,
        Adjustment::BrightnessContrast(_) => 3,
        Adjustment::ColorBalance(_) => 4,
    }
}

/// Contrast multiplier `k = tan(π/4 × (1 + c / 100))`, in f64, bounded.
pub(crate) fn contrast_slope(contrast: f32) -> f32 {
    let angle = std::f64::consts::FRAC_PI_4 * (1.0 + f64::from(contrast) / 100.0);
    if angle <= 0.0 {
        return 0.0;
    }
    angle.tan().clamp(0.0, MAX_CONTRAST_SLOPE) as f32
}

fn levels_words(channel: &LevelsChannel) -> [u32; 5] {
    [
        channel.input_black,
        channel.input_white,
        channel.gamma,
        channel.output_black,
        channel.output_white,
    ]
    .map(f32::to_bits)
}

fn shift_words(shift: &ColorShift) -> [u32; 3] {
    [shift.cyan_red, shift.magenta_green, shift.yellow_blue].map(|v| (v / 100.0).to_bits())
}

/// Curves read by `adjustment`, whose tables must be resident.
pub(crate) fn curves(adjustment: &Adjustment) -> Vec<&Curve> {
    match adjustment {
        Adjustment::Curves(c) => vec![&c.red, &c.green, &c.blue, &c.master],
        _ => Vec::new(),
    }
}

/// Words of `adjustment`, appended to `out`. The tables of its curves must
/// have been made resident by [`CurveTables::prepare`].
pub(crate) fn encode(
    adjustment: &Adjustment,
    tables: &CurveTables,
    out: &mut Vec<u32>,
) -> Result<(), crate::RenderError> {
    match adjustment {
        Adjustment::Levels(levels) => {
            for channel in [&levels.red, &levels.green, &levels.blue, &levels.master] {
                out.extend(levels_words(channel));
            }
        }
        Adjustment::Curves(_) => {
            for curve in curves(adjustment) {
                let table = tables.get(curve).ok_or(crate::RenderError::Unsupported(
                    "a curve table missing from the GPU",
                ))?;
                out.extend([
                    table.start.to_bits(),
                    table.end.to_bits(),
                    table.offset,
                    table.size,
                ]);
            }
        }
        Adjustment::HueSaturation(p) => {
            out.extend([p.hue / 360.0, p.saturation / 100.0, p.lightness / 100.0].map(f32::to_bits))
        }
        Adjustment::BrightnessContrast(p) => out.extend([
            (p.brightness / 100.0).to_bits(),
            contrast_slope(p.contrast).to_bits(),
        ]),
        Adjustment::ColorBalance(p) => {
            for shift in [&p.shadows, &p.midtones, &p.highlights] {
                out.extend(shift_words(shift));
            }
            out.extend([
                u32::from(p.preserve_luminosity),
                COLOR_BALANCE_STRENGTH.to_bits(),
            ]);
        }
    }
    Ok(())
}

/// Where a curve table lies in the table buffer.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PlacedTable {
    start: f32,
    end: f32,
    /// First value, in words.
    offset: u32,
    /// Number of values.
    size: u32,
}

/// Counters of the curve tables, for the benchmark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CurveTableStats {
    /// Tables computed and uploaded since the renderer was created.
    pub uploads: u64,
    /// Bytes uploaded for them.
    pub bytes: u64,
}

/// Curve lookup tables resident on the GPU, in one storage buffer. A table
/// is computed and uploaded once per curve content; when the tables of a
/// batch do not fit, the buffer is replaced by a larger one holding only
/// them.
pub(crate) struct CurveTables {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pub(crate) buffer: wgpu::Buffer,
    capacity: u32,
    used: u32,
    placed: HashMap<Vec<u32>, (PlacedTable, Arc<CurveTable>)>,
    pub(crate) stats: CurveTableStats,
}

/// Words of the smallest table buffer.
const MIN_TABLE_WORDS: u32 = 4 * 4096;

fn table_buffer(device: &wgpu::Device, words: u32) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("cygnus curve tables"),
        size: u64::from(words) * 4,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn key(curve: &Curve) -> Vec<u32> {
    curve
        .points
        .iter()
        .flat_map(|[x, y]| [x.to_bits(), y.to_bits()])
        .collect()
}

impl CurveTables {
    pub(crate) fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        Self {
            device: device.clone(),
            queue: queue.clone(),
            buffer: table_buffer(device, MIN_TABLE_WORDS),
            capacity: MIN_TABLE_WORDS,
            used: 0,
            placed: HashMap::new(),
            stats: CurveTableStats::default(),
        }
    }

    /// Makes the tables of `curves` resident: missing ones are computed and
    /// uploaded after the resident ones; if they do not fit, the buffer is
    /// replaced by one holding only the tables of `curves` (tables read by
    /// earlier submitted work stay in the old buffer, kept alive by wgpu).
    pub(crate) fn prepare(&mut self, curves: &[&Curve]) {
        let mut missing: Vec<(Vec<u32>, Arc<CurveTable>)> = Vec::new();
        for curve in curves {
            let key = key(curve);
            if !self.placed.contains_key(&key) && !missing.iter().any(|(k, _)| *k == key) {
                missing.push((key, Arc::new(curve.lookup_table())));
            }
        }
        if missing.is_empty() {
            return;
        }
        let words = |tables: &[(Vec<u32>, Arc<CurveTable>)]| -> u32 {
            tables.iter().map(|(_, t)| t.values.len() as u32).sum()
        };
        if self.used.saturating_add(words(&missing)) > self.capacity {
            // Keep only the tables of this batch.
            let mut kept: Vec<(Vec<u32>, Arc<CurveTable>)> = Vec::new();
            for curve in curves {
                let key = key(curve);
                if let Some((_, table)) = self.placed.get(&key)
                    && !kept.iter().any(|(k, _)| *k == key)
                {
                    kept.push((key, table.clone()));
                }
            }
            kept.extend(missing);
            missing = kept;
            let capacity = words(&missing).max(self.capacity.saturating_mul(2));
            self.buffer = table_buffer(&self.device, capacity);
            self.capacity = capacity;
            self.used = 0;
            self.placed.clear();
        }
        for (key, table) in missing {
            let size = table.values.len() as u32;
            let placed = PlacedTable {
                start: table.start,
                end: table.end,
                offset: self.used,
                size,
            };
            self.queue.write_buffer(
                &self.buffer,
                u64::from(self.used) * 4,
                bytemuck::cast_slice(&table.values),
            );
            self.used += size;
            self.stats.uploads += 1;
            self.stats.bytes += u64::from(size) * 4;
            self.placed.insert(key, (placed, table));
        }
    }

    /// Where the table of `curve` lies, if resident.
    pub(crate) fn get(&self, curve: &Curve) -> Option<PlacedTable> {
        self.placed.get(&key(curve)).map(|(placed, _)| *placed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contrast_slope_spans_flat_to_threshold() {
        assert_eq!(contrast_slope(-100.0), 0.0);
        assert!((contrast_slope(0.0) - 1.0).abs() < 1e-6);
        assert!((contrast_slope(50.0) - 2.414_213_5).abs() < 1e-5);
        assert_eq!(contrast_slope(100.0), MAX_CONTRAST_SLOPE as f32);
    }

    #[test]
    fn kinds_match_their_codes() {
        use cygnus_photo_core::adjust::*;
        let all = [
            Adjustment::Levels(Levels::default()),
            Adjustment::Curves(Curves::default()),
            Adjustment::HueSaturation(HueSaturation::default()),
            Adjustment::BrightnessContrast(BrightnessContrast::default()),
            Adjustment::ColorBalance(ColorBalance::default()),
        ];
        for (adjustment, (_, code)) in all.iter().zip(KINDS) {
            assert_eq!(kind(adjustment), code);
        }
    }
}

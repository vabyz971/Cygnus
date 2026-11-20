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

//! Brush strokes (docs/decisions/0035 and 0036): the `brush.stroke`
//! command and the positions of its dabs.
//!
//! A stroke is a list of input points. The path passes through them (a
//! centripetal Catmull-Rom spline) and a dab is laid every `spacing ×
//! diameter` along it, measured by arc length, carried over from one
//! segment to the next. Pressure is interpolated linearly between points
//! and drives the diameter and the flow. This is a pure function in f64:
//! the same points give the same dabs on every platform, which the GPU
//! then renders (`photo/render`), through the resolver of
//! [`crate::resolve`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::commands::CoreError;
use crate::model::{BlendMode, NodeId, Rect};
use crate::pixels::TileBackup;

/// Points of a stroke at most: one command cannot saturate the memory.
pub const MAX_POINTS: usize = 100_000;
/// Dabs of a stroke at most.
pub const MAX_DABS: usize = 1_000_000;
/// Smallest and largest brush diameter, in pixels.
pub const MIN_DIAMETER: f32 = 1.0;
/// Largest brush diameter, in pixels.
pub const MAX_DIAMETER: f32 = 5000.0;
/// Smallest distance between two dabs, in pixels, whatever the spacing.
pub const MIN_STEP: f64 = 0.25;
/// Largest coordinate of a point, in pixels.
const MAX_COORDINATE: f64 = 1.0e7;
/// Pieces of a spline segment per pixel of chord, to measure arc length.
const PIECES_PER_PIXEL: f64 = 4.0;
/// Pieces of one spline segment at most.
const MAX_PIECES: f64 = 65_536.0;

/// One input point of a stroke, in document pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StrokePoint {
    /// Horizontal position.
    pub x: f64,
    /// Vertical position.
    pub y: f64,
    /// Pen pressure, 0 to 1; absent for a mouse (full pressure).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pressure: Option<f32>,
    /// Pen tilt along x, in degrees (recorded, not used yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt_x: Option<f32>,
    /// Pen tilt along y, in degrees (recorded, not used yet).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tilt_y: Option<f32>,
    /// Time since the start of the stroke, in milliseconds (recorded).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_ms: Option<f64>,
}

/// What the stroke does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BrushTool {
    /// Paints the brush color.
    #[default]
    Brush,
}

fn default_hardness() -> f32 {
    0.8
}
fn default_spacing() -> f32 {
    0.1
}
fn one() -> f32 {
    1.0
}
fn default_min_size() -> f32 {
    0.2
}
fn yes() -> bool {
    true
}

/// Settings of the brush.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Brush {
    /// Diameter in pixels, 1 to 5000.
    pub diameter: f32,
    /// Part of the radius painted at full coverage, 0 (soft) to 1 (hard).
    #[serde(default = "default_hardness")]
    pub hardness: f32,
    /// Distance between dabs, in diameters, 0.01 to 2.
    #[serde(default = "default_spacing")]
    pub spacing: f32,
    /// Coverage laid by each dab, 0 to 1.
    #[serde(default = "one")]
    pub flow: f32,
    /// Largest coverage of the whole stroke, 0 to 1.
    #[serde(default = "one")]
    pub opacity: f32,
    /// How the stroke blends with the layer.
    #[serde(default)]
    pub mode: BlendMode,
    /// Straight RGBA color, sRGB-encoded, 0 to 1.
    pub color: [f32; 4],
    /// Pressure scales the diameter, down to `min_size`.
    #[serde(default = "yes")]
    pub size_pressure: bool,
    /// Diameter at zero pressure, as a part of `diameter`, 0 to 1.
    #[serde(default = "default_min_size")]
    pub min_size: f32,
    /// Pressure scales the flow.
    #[serde(default = "yes")]
    pub flow_pressure: bool,
}

impl Brush {
    /// The default brush of diameter 20 painting `color`.
    pub fn new(color: [f32; 4]) -> Self {
        Self {
            diameter: 20.0,
            hardness: default_hardness(),
            spacing: default_spacing(),
            flow: 1.0,
            opacity: 1.0,
            mode: BlendMode::Normal,
            color,
            size_pressure: true,
            min_size: default_min_size(),
            flow_pressure: true,
        }
    }

    /// Checks every setting against its range.
    pub fn validate(&self) -> Result<(), CoreError> {
        let unit = |param: &'static str, value: f32| {
            if value.is_finite() && (0.0..=1.0).contains(&value) {
                Ok(())
            } else {
                Err(invalid(param, format!("{value} is not in 0..=1")))
            }
        };
        if !(self.diameter.is_finite() && (MIN_DIAMETER..=MAX_DIAMETER).contains(&self.diameter)) {
            return Err(invalid(
                "brush.diameter",
                format!("{} is not in 1..=5000", self.diameter),
            ));
        }
        if !(self.spacing.is_finite() && (0.01..=2.0).contains(&self.spacing)) {
            return Err(invalid(
                "brush.spacing",
                format!("{} is not in 0.01..=2", self.spacing),
            ));
        }
        unit("brush.hardness", self.hardness)?;
        unit("brush.flow", self.flow)?;
        unit("brush.opacity", self.opacity)?;
        unit("brush.min_size", self.min_size)?;
        for value in self.color {
            unit("brush.color", value)?;
        }
        Ok(())
    }
}

/// Parameters of `brush.stroke`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BrushStroke {
    /// Pixel layer painted.
    pub target: NodeId,
    /// What the stroke does.
    #[serde(default)]
    pub tool: BrushTool,
    /// Input points, already stabilized, 1 to 100 000.
    pub points: Vec<StrokePoint>,
    /// Brush settings.
    pub brush: Brush,
    /// The painted tiles of the target, set by the resolver (never
    /// serialized): their new content, `None` where a tile becomes empty.
    #[serde(skip)]
    #[schemars(skip)]
    pub resolved: Option<TileBackup>,
}

/// One dab: its center in document pixels, its radius and the coverage it
/// lays (flow, after pressure).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dab {
    /// Center, horizontal.
    pub x: f64,
    /// Center, vertical.
    pub y: f64,
    /// Radius in pixels.
    pub radius: f64,
    /// Coverage laid, 0 to 1.
    pub flow: f64,
}

impl Dab {
    /// Whether the dab reaches `rect`.
    pub fn touches(&self, rect: &Rect) -> bool {
        let reach = self.radius + 1.0;
        self.x + reach > f64::from(rect.x)
            && self.x - reach < f64::from(rect.x) + f64::from(rect.width)
            && self.y + reach > f64::from(rect.y)
            && self.y - reach < f64::from(rect.y) + f64::from(rect.height)
    }
}

fn invalid(param: &'static str, reason: impl Into<String>) -> CoreError {
    CoreError::InvalidValue {
        param,
        reason: reason.into(),
    }
}

/// A point of the path: position and pressure.
#[derive(Debug, Clone, Copy)]
struct Sample {
    x: f64,
    y: f64,
    pressure: f64,
}

fn distance(a: Sample, b: Sample) -> f64 {
    ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()
}

/// Point at `u` (0..1) of the centripetal Catmull-Rom segment from `p1` to
/// `p2` (Barry and Goldman's formulation, alpha 0.5).
fn catmull_rom(p0: Sample, p1: Sample, p2: Sample, p3: Sample, u: f64) -> (f64, f64) {
    let knot = |a: Sample, b: Sample| distance(a, b).sqrt().max(1e-12);
    let t0 = 0.0;
    let t1 = t0 + knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);
    let t = t1 + u * (t2 - t1);
    let lerp = |a: (f64, f64), b: (f64, f64), ta: f64, tb: f64| {
        let (wa, wb) = ((tb - t) / (tb - ta), (t - ta) / (tb - ta));
        (a.0 * wa + b.0 * wb, a.1 * wa + b.1 * wb)
    };
    let (q0, q1, q2, q3) = ((p0.x, p0.y), (p1.x, p1.y), (p2.x, p2.y), (p3.x, p3.y));
    let a1 = lerp(q0, q1, t0, t1);
    let a2 = lerp(q1, q2, t1, t2);
    let a3 = lerp(q2, q3, t2, t3);
    let b1 = lerp(a1, a2, t0, t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

/// The path through `points` as a dense polyline.
fn path(points: &[Sample]) -> Vec<Sample> {
    let mut out = vec![points[0]];
    for i in 0..points.len().saturating_sub(1) {
        let (p1, p2) = (points[i], points[i + 1]);
        // Missing neighbours at the ends are mirrored: a straight segment
        // stays straight.
        let p0 = if i > 0 {
            points[i - 1]
        } else {
            Sample {
                x: 2.0 * p1.x - p2.x,
                y: 2.0 * p1.y - p2.y,
                ..p1
            }
        };
        let p3 = if i + 2 < points.len() {
            points[i + 2]
        } else {
            Sample {
                x: 2.0 * p2.x - p1.x,
                y: 2.0 * p2.y - p1.y,
                ..p2
            }
        };
        let pieces = (distance(p1, p2) * PIECES_PER_PIXEL)
            .ceil()
            .clamp(1.0, MAX_PIECES) as u32;
        for k in 1..=pieces {
            let u = f64::from(k) / f64::from(pieces);
            let (x, y) = if k == pieces {
                (p2.x, p2.y)
            } else {
                catmull_rom(p0, p1, p2, p3, u)
            };
            out.push(Sample {
                x,
                y,
                pressure: p1.pressure + (p2.pressure - p1.pressure) * u,
            });
        }
    }
    out
}

/// The dabs of a stroke of `points` with `brush`, in order. Dabs that lay
/// nothing (zero flow) are left out.
pub fn dabs(points: &[StrokePoint], brush: &Brush) -> Result<Vec<Dab>, CoreError> {
    brush.validate()?;
    if points.is_empty() || points.len() > MAX_POINTS {
        return Err(invalid(
            "points",
            format!("{} points given, 1 to {MAX_POINTS} expected", points.len()),
        ));
    }
    let mut samples: Vec<Sample> = Vec::with_capacity(points.len());
    for point in points {
        let finite = point.x.is_finite()
            && point.y.is_finite()
            && point.x.abs() <= MAX_COORDINATE
            && point.y.abs() <= MAX_COORDINATE;
        if !finite {
            return Err(invalid("points", "coordinates must be finite and in range"));
        }
        let pressure = point.pressure.unwrap_or(1.0);
        if !(pressure.is_finite() && (0.0..=1.0).contains(&pressure)) {
            return Err(invalid(
                "points",
                format!("pressure {pressure} is not in 0..=1"),
            ));
        }
        let sample = Sample {
            x: point.x,
            y: point.y,
            pressure: f64::from(pressure),
        };
        // A repeated position adds nothing to the path.
        match samples.last_mut() {
            Some(last) if last.x == sample.x && last.y == sample.y => *last = sample,
            _ => samples.push(sample),
        }
    }
    let size = |pressure: f64| {
        let factor = if brush.size_pressure {
            f64::from(brush.min_size) + (1.0 - f64::from(brush.min_size)) * pressure
        } else {
            1.0
        };
        f64::from(brush.diameter) * factor
    };
    let flow =
        |pressure: f64| f64::from(brush.flow) * if brush.flow_pressure { pressure } else { 1.0 };
    let step = |pressure: f64| (f64::from(brush.spacing) * size(pressure)).max(MIN_STEP);

    let line = path(&samples);
    let mut out = Vec::new();
    let place = |sample: Sample, out: &mut Vec<Dab>| -> Result<(), CoreError> {
        let dab = Dab {
            x: sample.x,
            y: sample.y,
            radius: size(sample.pressure) / 2.0,
            flow: flow(sample.pressure),
        };
        if dab.flow > 0.0 {
            if out.len() == MAX_DABS {
                return Err(invalid(
                    "points",
                    format!("the stroke would lay more than {MAX_DABS} dabs"),
                ));
            }
            out.push(dab);
        }
        Ok(())
    };
    place(line[0], &mut out)?;
    // Distance left to walk before the next dab.
    let mut left = step(line[0].pressure);
    for pair in line.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let length = distance(a, b);
        let mut walked = 0.0;
        // A hair of tolerance: a dab due at the very end of a piece is laid.
        while length - walked >= left - 1e-9 {
            walked += left;
            let u = walked / length;
            let sample = Sample {
                x: a.x + (b.x - a.x) * u,
                y: a.y + (b.y - a.y) * u,
                pressure: a.pressure + (b.pressure - a.pressure) * u,
            };
            place(sample, &mut out)?;
            left = step(sample.pressure);
        }
        left -= length - walked;
    }
    Ok(out)
}

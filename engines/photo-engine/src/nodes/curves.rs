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

//! Curves (O005): master RGB tone curve through user control points,
//! interpolated with a monotone cubic (Fritsch-Carlson: no overshoot,
//! a rising curve never folds back), baked into a 256-entry LUT.
//!
//! Points travel as a `Text` parameter (`"x,y;x,y;…"`, 8-bit scale)
//! so the generic param map and the `.cygp` format carry them without
//! a model change. A missing or unreadable value falls back to the
//! identity diagonal.

use super::levels::apply_rgb_lut;
use super::{Effect, NodeCtx};
use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};
use image::DynamicImage;

/// Registry id.
pub const TYPE_ID: &str = "curves";
/// Text parameter key holding the control points.
pub const POINTS_KEY: &str = "points";
/// Identity diagonal (default value of [`POINTS_KEY`]).
pub const IDENTITY: &str = "0,0;255,255";
/// Upper bound on control points (editor and parser alike).
pub const MAX_POINTS: usize = 16;

pub fn definition() -> NodeDefinition {
    NodeDefinition::new(TYPE_ID, "Curves", NodeCategory::Color)
        .input(SocketDef::new("image", "Image", SocketType::Image))
        .output(SocketDef::new("image", "Image", SocketType::Image))
        .param(POINTS_KEY, ParamValue::Text(IDENTITY.to_owned()))
        .header_color([0.45, 0.55, 0.70])
        .description("RGB tone curve through control points")
}

/// Parses `"x,y;x,y"` into points sorted by x, clamped to 0..=255,
/// one point per x (last wins). Fewer than two valid points → the
/// identity diagonal.
#[must_use]
pub fn parse_points(text: &str) -> Vec<[f32; 2]> {
    let mut points: Vec<[f32; 2]> = text
        .split(';')
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            let x: f32 = x.trim().parse().ok()?;
            let y: f32 = y.trim().parse().ok()?;
            (x.is_finite() && y.is_finite()).then_some([x.clamp(0.0, 255.0), y.clamp(0.0, 255.0)])
        })
        .take(MAX_POINTS)
        .collect();
    points.sort_by(|a, b| a[0].total_cmp(&b[0]));
    points.dedup_by(|later, earlier| {
        let same = (later[0] - earlier[0]).abs() < 0.5;
        if same {
            *earlier = *later;
        }
        same
    });
    if points.len() < 2 {
        return vec![[0.0, 0.0], [255.0, 255.0]];
    }
    points
}

/// Inverse of [`parse_points`] (one decimal, stable round-trip).
#[must_use]
pub fn format_points(points: &[[f32; 2]]) -> String {
    points
        .iter()
        .map(|[x, y]| format!("{},{}", round1(*x), round1(*y)))
        .collect::<Vec<_>>()
        .join(";")
}

fn round1(v: f32) -> f32 {
    (v * 10.0).round() / 10.0
}

/// Evaluates the monotone cubic through `points` (sorted, >= 2) at
/// every 8-bit input. Outside the first/last point the curve is flat.
#[must_use]
pub fn evaluate(points: &[[f32; 2]]) -> [f32; 256] {
    let mut out = [0.0f32; 256];
    let n = points.len();
    if n < 2 {
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = i as f32;
        }
        return out;
    }
    // Secants then Fritsch-Carlson tangents.
    let mut delta = vec![0.0f32; n - 1];
    for k in 0..n - 1 {
        let dx = (points[k + 1][0] - points[k][0]).max(1e-3);
        delta[k] = (points[k + 1][1] - points[k][1]) / dx;
    }
    let mut m = vec![0.0f32; n];
    m[0] = delta[0];
    m[n - 1] = delta[n - 2];
    for k in 1..n - 1 {
        m[k] = if delta[k - 1] * delta[k] <= 0.0 {
            0.0
        } else {
            (delta[k - 1] + delta[k]) / 2.0
        };
    }
    for k in 0..n - 1 {
        if delta[k].abs() < 1e-6 {
            m[k] = 0.0;
            m[k + 1] = 0.0;
            continue;
        }
        let a = m[k] / delta[k];
        let b = m[k + 1] / delta[k];
        let s = a * a + b * b;
        if s > 9.0 {
            let t = 3.0 / s.sqrt();
            m[k] = t * a * delta[k];
            m[k + 1] = t * b * delta[k];
        }
    }
    for (i, slot) in out.iter_mut().enumerate() {
        let x = i as f32;
        *slot = if x <= points[0][0] {
            points[0][1]
        } else if x >= points[n - 1][0] {
            points[n - 1][1]
        } else {
            let k = points
                .windows(2)
                .position(|w| x >= w[0][0] && x <= w[1][0])
                .unwrap_or(0);
            let (x0, y0) = (points[k][0], points[k][1]);
            let (x1, y1) = (points[k + 1][0], points[k + 1][1]);
            let h = (x1 - x0).max(1e-3);
            let t = (x - x0) / h;
            let t2 = t * t;
            let t3 = t2 * t;
            (2.0 * t3 - 3.0 * t2 + 1.0) * y0
                + (t3 - 2.0 * t2 + t) * h * m[k]
                + (-2.0 * t3 + 3.0 * t2) * y1
                + (t3 - t2) * h * m[k + 1]
        }
        .clamp(0.0, 255.0);
    }
    out
}

/// 8-bit LUT of the curve.
#[must_use]
pub fn lut(points: &[[f32; 2]]) -> [u8; 256] {
    let curve = evaluate(points);
    let mut lut = [0u8; 256];
    for (slot, v) in lut.iter_mut().zip(curve.iter()) {
        *slot = v.round() as u8;
    }
    lut
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    let input = ctx.input()?;
    let text = match ctx.params.get(POINTS_KEY) {
        Some(ParamValue::Text(text)) => text.as_str(),
        _ => IDENTITY,
    };
    Some(apply_rgb_lut(input, &lut(&parse_points(text))))
}

pub fn effect() -> Effect {
    Effect {
        definition: definition(),
        apply,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nodes::levels::is_identity;

    #[test]
    fn identity_points_give_identity_lut() {
        assert!(is_identity(&lut(&parse_points(IDENTITY))));
    }

    #[test]
    fn garbage_falls_back_to_identity() {
        assert_eq!(
            parse_points("n'importe quoi"),
            vec![[0.0, 0.0], [255.0, 255.0]]
        );
        assert_eq!(parse_points(""), vec![[0.0, 0.0], [255.0, 255.0]]);
    }

    #[test]
    fn parse_sorts_clamps_and_dedups() {
        let points = parse_points("255,300;0,-5;128,100;128,140");
        assert_eq!(points, vec![[0.0, 0.0], [128.0, 140.0], [255.0, 255.0]]);
    }

    #[test]
    fn format_round_trips() {
        let points = vec![[0.0, 10.0], [64.5, 50.0], [255.0, 240.0]];
        assert_eq!(parse_points(&format_points(&points)), points);
    }

    #[test]
    fn curve_passes_through_control_points() {
        let points = parse_points("0,0;64,40;192,215;255,255");
        let l = lut(&points);
        assert_eq!(l[64], 40);
        assert_eq!(l[192], 215);
        assert_eq!(l[0], 0);
        assert_eq!(l[255], 255);
    }

    #[test]
    fn rising_points_give_monotone_curve() {
        let curve = evaluate(&parse_points("0,0;30,120;60,130;255,255"));
        for w in curve.windows(2) {
            assert!(w[1] + 1e-3 >= w[0], "courbe non monotone : {w:?}");
        }
    }

    #[test]
    fn inverted_curve() {
        let l = lut(&parse_points("0,255;255,0"));
        assert_eq!(l[0], 255);
        assert_eq!(l[255], 0);
        assert!((i16::from(l[128]) - 127).abs() <= 1);
    }

    #[test]
    fn flat_outside_end_points() {
        let l = lut(&parse_points("20,30;230,200"));
        assert_eq!(l[0], 30);
        assert_eq!(l[10], 30);
        assert_eq!(l[255], 200);
    }
}

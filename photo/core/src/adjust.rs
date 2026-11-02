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

//! Parameters of adjustment layers (REG-1) and filter layers, with their
//! validation (docs/decisions/0020).
//!
//! Values act on sRGB-encoded colors (0021, Q5). The curve shape is defined
//! here once: the renderer bakes it into a lookup table, the Adjustments
//! panel draws it, both through [`Curve::evaluate`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

fn check(ok: bool, what: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what.to_owned()) }
}

fn in_range(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && (min..=max).contains(&value)
}

/// Levels of one channel, values from 0 to 1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LevelsChannel {
    /// Input value mapped to black.
    pub input_black: f32,
    /// Input value mapped to white.
    pub input_white: f32,
    /// Midtone gamma, from 0.1 to 10 (1 leaves midtones unchanged).
    pub gamma: f32,
    /// Output value of black.
    pub output_black: f32,
    /// Output value of white.
    pub output_white: f32,
}

impl Default for LevelsChannel {
    fn default() -> Self {
        Self {
            input_black: 0.0,
            input_white: 1.0,
            gamma: 1.0,
            output_black: 0.0,
            output_white: 1.0,
        }
    }
}

impl LevelsChannel {
    fn validate(&self) -> Result<(), String> {
        for value in [
            self.input_black,
            self.input_white,
            self.output_black,
            self.output_white,
        ] {
            check(in_range(value, 0.0, 1.0), "levels values must be in 0..=1")?;
        }
        check(
            self.input_black < self.input_white,
            "input black must be below input white",
        )?;
        check(
            in_range(self.gamma, 0.1, 10.0),
            "levels gamma must be in 0.1..=10",
        )
    }
}

/// Levels adjustment. Each color channel goes through its own levels
/// first, then through the master levels, as with curves.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Levels {
    /// Applied to the three color channels, after the channel levels.
    pub master: LevelsChannel,
    /// Red channel.
    pub red: LevelsChannel,
    /// Green channel.
    pub green: LevelsChannel,
    /// Blue channel.
    pub blue: LevelsChannel,
}

/// A tone curve: 2 to 16 points, x strictly increasing, all in 0..=1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Curve {
    /// Control points `[x, y]`.
    pub points: Vec<[f32; 2]>,
}

impl Default for Curve {
    fn default() -> Self {
        Self {
            points: vec![[0.0, 0.0], [1.0, 1.0]],
        }
    }
}

/// Largest number of points of a curve.
pub const MAX_CURVE_POINTS: usize = 16;

impl Curve {
    fn validate(&self) -> Result<(), String> {
        check(
            (2..=MAX_CURVE_POINTS).contains(&self.points.len()),
            "a curve has 2 to 16 points",
        )?;
        for [x, y] in &self.points {
            check(
                in_range(*x, 0.0, 1.0) && in_range(*y, 0.0, 1.0),
                "curve points must be in 0..=1",
            )?;
        }
        check(
            self.points.windows(2).all(|pair| pair[0][0] < pair[1][0]),
            "curve points must have strictly increasing x",
        )
    }

    /// Slopes at the points for a monotone cubic interpolation
    /// (Fritsch and Carlson, 1980): the curve never overshoots its points.
    fn tangents(&self) -> Vec<f32> {
        let p = &self.points;
        let n = p.len();
        if n < 2 {
            return vec![0.0; n];
        }
        let secant: Vec<f32> = p
            .windows(2)
            .map(|w| (w[1][1] - w[0][1]) / (w[1][0] - w[0][0]))
            .collect();
        let mut m = vec![0.0; n];
        m[0] = secant[0];
        m[n - 1] = secant[n - 2];
        for i in 1..n - 1 {
            m[i] = if secant[i - 1] * secant[i] <= 0.0 {
                0.0
            } else {
                (secant[i - 1] + secant[i]) / 2.0
            };
        }
        for i in 0..n - 1 {
            if secant[i] == 0.0 {
                m[i] = 0.0;
                m[i + 1] = 0.0;
                continue;
            }
            let a = m[i] / secant[i];
            let b = m[i + 1] / secant[i];
            let s = a * a + b * b;
            if s > 9.0 {
                let t = 3.0 / s.sqrt();
                m[i] = t * a * secant[i];
                m[i + 1] = t * b * secant[i];
            }
        }
        m
    }

    fn shape(&self) -> Shape<'_> {
        Shape {
            points: &self.points,
            tangents: self.tangents(),
        }
    }

    /// Value of the curve at `x` (0..=1). Flat outside the first and last
    /// points. This is the only definition of the curve shape.
    pub fn evaluate(&self, x: f32) -> f32 {
        self.shape().evaluate(x)
    }

    /// The curve sampled at `size` evenly spaced inputs from 0 to 1.
    pub fn table(&self, size: usize) -> Vec<f32> {
        let shape = self.shape();
        let last = size.saturating_sub(1).max(1) as f32;
        (0..size).map(|i| shape.evaluate(i as f32 / last)).collect()
    }

    /// Lookup table read by the renderer with linear interpolation
    /// (docs/decisions/0028). It covers the domain of the cubic part, from
    /// the first to the last point: the renderer bounds its input to that
    /// domain, where the curve is flat outside, so the table has no kink.
    /// Its size is the smallest power of two from [`MIN_CURVE_TABLE`] whose
    /// interpolation error stays within [`CURVE_TABLE_TOLERANCE`], at most
    /// [`MAX_CURVE_TABLE`]. The error of linear interpolation over a step
    /// `h` is at most `h² / 8 × max |f''|`, and `f''` is linear on each
    /// cubic segment, so its maximum is at an end of a segment.
    pub fn lookup_table(&self) -> CurveTable {
        let shape = self.shape();
        let (Some(first), Some(last)) = (self.points.first(), self.points.last()) else {
            return CurveTable {
                start: 0.0,
                end: 1.0,
                values: vec![0.0, 1.0],
            };
        };
        let (start, end) = (first[0], last[0]);
        let curvature = shape.max_second_derivative();
        let mut size = MIN_CURVE_TABLE;
        while size < MAX_CURVE_TABLE {
            let step = f64::from(end - start) / (size - 1) as f64;
            if step * step / 8.0 * curvature <= CURVE_TABLE_TOLERANCE {
                break;
            }
            size *= 2;
        }
        let values = if start == 0.0 && end == 1.0 {
            // Same inputs as `table`, so that a full table holds exactly
            // the values of every 16-bit level.
            let last = (size - 1) as f32;
            (0..size).map(|i| shape.evaluate(i as f32 / last)).collect()
        } else {
            let last = (size - 1) as f64;
            let span = f64::from(end - start);
            (0..size)
                .map(|i| shape.evaluate((f64::from(start) + span * i as f64 / last) as f32))
                .collect()
        };
        CurveTable { start, end, values }
    }
}

/// Smallest size of a curve lookup table.
pub const MIN_CURVE_TABLE: usize = 4096;
/// Largest size of a curve lookup table: one entry per 16-bit level.
pub const MAX_CURVE_TABLE: usize = 65536;
/// Largest interpolation error of a curve lookup table, a quarter of a
/// 16-bit unit (docs/decisions/0028).
pub const CURVE_TABLE_TOLERANCE: f64 = 0.25 / 65535.0;

/// A curve sampled for the renderer (see [`Curve::lookup_table`]).
#[derive(Debug, Clone, PartialEq)]
pub struct CurveTable {
    /// Input of the first value, the x of the first point.
    pub start: f32,
    /// Input of the last value, the x of the last point.
    pub end: f32,
    /// Values at evenly spaced inputs from `start` to `end`.
    pub values: Vec<f32>,
}

/// Points of a curve with their slopes.
struct Shape<'a> {
    points: &'a [[f32; 2]],
    tangents: Vec<f32>,
}

impl Shape<'_> {
    fn evaluate(&self, x: f32) -> f32 {
        let p = self.points;
        let m = &self.tangents;
        let (Some(first), Some(last)) = (p.first(), p.last()) else {
            return x;
        };
        if p.len() < 2 || x <= first[0] {
            return first[1];
        }
        if x >= last[0] {
            return last[1];
        }
        let i = p
            .windows(2)
            .position(|w| x < w[1][0])
            .unwrap_or(p.len() - 2);
        let (x0, y0, x1, y1) = (p[i][0], p[i][1], p[i + 1][0], p[i + 1][1]);
        let h = x1 - x0;
        let t = (x - x0) / h;
        let (t2, t3) = (t * t, t * t * t);
        let value = (2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * h * m[i]
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * h * m[i + 1];
        value.clamp(0.0, 1.0)
    }

    /// Largest |f''| over the cubic segments, at their ends, in x units.
    fn max_second_derivative(&self) -> f64 {
        let p = self.points;
        let m = &self.tangents;
        (0..p.len().saturating_sub(1))
            .map(|i| {
                let h = f64::from(p[i + 1][0] - p[i][0]);
                let (y0, y1) = (f64::from(p[i][1]), f64::from(p[i + 1][1]));
                let (d0, d1) = (h * f64::from(m[i]), h * f64::from(m[i + 1]));
                // Second derivative of the Hermite basis in t at t = 0 and 1.
                let at0 = 6.0 * (y1 - y0) - 4.0 * d0 - 2.0 * d1;
                let at1 = -6.0 * (y1 - y0) + 2.0 * d0 + 4.0 * d1;
                at0.abs().max(at1.abs()) / (h * h)
            })
            .fold(0.0, f64::max)
    }
}

/// Curves adjustment (REG-3). Each color channel goes through its own
/// curve first, then through the master curve.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Curves {
    /// Applied to the three color channels, after the channel curves.
    pub master: Curve,
    /// Red channel.
    pub red: Curve,
    /// Green channel.
    pub green: Curve,
    /// Blue channel.
    pub blue: Curve,
}

/// Hue/saturation adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct HueSaturation {
    /// Hue shift in degrees, from -180 to 180.
    pub hue: f32,
    /// Saturation change, from -100 to 100.
    pub saturation: f32,
    /// Lightness change, from -100 to 100.
    pub lightness: f32,
}

/// Brightness/contrast adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct BrightnessContrast {
    /// From -100 to 100.
    pub brightness: f32,
    /// From -100 to 100.
    pub contrast: f32,
}

/// Color shift of one tonal range, each axis from -100 to 100.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct ColorShift {
    /// Cyan (negative) to red (positive).
    pub cyan_red: f32,
    /// Magenta (negative) to green (positive).
    pub magenta_green: f32,
    /// Yellow (negative) to blue (positive).
    pub yellow_blue: f32,
}

/// Color balance adjustment.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ColorBalance {
    /// Shift of the shadows.
    pub shadows: ColorShift,
    /// Shift of the midtones.
    pub midtones: ColorShift,
    /// Shift of the highlights.
    pub highlights: ColorShift,
    /// Keep the luminosity of each pixel.
    pub preserve_luminosity: bool,
}

impl Default for ColorBalance {
    fn default() -> Self {
        Self {
            shadows: ColorShift::default(),
            midtones: ColorShift::default(),
            highlights: ColorShift::default(),
            preserve_luminosity: true,
        }
    }
}

/// An adjustment and its parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Adjustment {
    /// Levels.
    Levels(Levels),
    /// Curves.
    Curves(Curves),
    /// Hue/saturation.
    HueSaturation(HueSaturation),
    /// Brightness/contrast.
    BrightnessContrast(BrightnessContrast),
    /// Color balance.
    ColorBalance(ColorBalance),
}

impl Adjustment {
    /// Checks every parameter against its range.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Levels(levels) => {
                for channel in [&levels.master, &levels.red, &levels.green, &levels.blue] {
                    channel.validate()?;
                }
                Ok(())
            }
            Self::Curves(curves) => {
                for curve in [&curves.master, &curves.red, &curves.green, &curves.blue] {
                    curve.validate()?;
                }
                Ok(())
            }
            Self::HueSaturation(h) => {
                check(in_range(h.hue, -180.0, 180.0), "hue must be in -180..=180")?;
                check(
                    in_range(h.saturation, -100.0, 100.0) && in_range(h.lightness, -100.0, 100.0),
                    "saturation and lightness must be in -100..=100",
                )
            }
            Self::BrightnessContrast(b) => check(
                in_range(b.brightness, -100.0, 100.0) && in_range(b.contrast, -100.0, 100.0),
                "brightness and contrast must be in -100..=100",
            ),
            Self::ColorBalance(c) => {
                for shift in [c.shadows, c.midtones, c.highlights] {
                    for value in [shift.cyan_red, shift.magenta_green, shift.yellow_blue] {
                        check(
                            in_range(value, -100.0, 100.0),
                            "color balance values must be in -100..=100",
                        )?;
                    }
                }
                Ok(())
            }
        }
    }
}

/// Largest filter radius of the beta, in pixels (0023, Q6).
pub const MAX_FILTER_RADIUS: f32 = 100.0;

/// A neighbourhood filter and its parameters.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Filter {
    /// Gaussian blur; `radius` is the standard deviation, 0.1 to 100 px.
    GaussianBlur {
        /// Standard deviation in document pixels.
        radius: f32,
    },
    /// Unsharp mask.
    UnsharpMask {
        /// Blur radius (standard deviation), 0.1 to 100 px.
        radius: f32,
        /// Strength, 0 to 5 (0 to 500 %).
        amount: f32,
        /// Smallest difference sharpened, 0 to 255 levels.
        threshold: f32,
    },
}

impl Filter {
    /// Checks every parameter against its range.
    pub fn validate(&self) -> Result<(), String> {
        let radius_ok = |r: f32| in_range(r, 0.1, MAX_FILTER_RADIUS);
        match *self {
            Self::GaussianBlur { radius } => {
                check(radius_ok(radius), "radius must be in 0.1..=100")
            }
            Self::UnsharpMask {
                radius,
                amount,
                threshold,
            } => {
                check(radius_ok(radius), "radius must be in 0.1..=100")?;
                check(in_range(amount, 0.0, 5.0), "amount must be in 0..=5")?;
                check(
                    in_range(threshold, 0.0, 255.0),
                    "threshold must be in 0..=255",
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identity_curve_is_identity() {
        let curve = Curve::default();
        for i in 0..=10 {
            let x = i as f32 / 10.0;
            assert!((curve.evaluate(x) - x).abs() < 1e-6);
        }
    }

    #[test]
    fn curve_passes_through_its_points_and_never_overshoots() {
        let curve = Curve {
            points: vec![[0.0, 0.0], [0.25, 0.1], [0.5, 0.9], [1.0, 1.0]],
        };
        for [x, y] in &curve.points {
            assert!((curve.evaluate(*x) - y).abs() < 1e-6);
        }
        // Monotone data gives a monotone curve.
        let table = curve.table(1024);
        assert!(table.windows(2).all(|pair| pair[1] >= pair[0] - 1e-6));
        assert!(table.iter().all(|v| (0.0..=1.0).contains(v)));
    }

    #[test]
    fn curve_is_flat_outside_its_points() {
        let curve = Curve {
            points: vec![[0.2, 0.3], [0.8, 0.6]],
        };
        assert_eq!(curve.evaluate(0.0), 0.3);
        assert_eq!(curve.evaluate(1.0), 0.6);
    }

    #[test]
    fn invalid_parameters_are_reported() {
        let bad_curve = Adjustment::Curves(Curves {
            master: Curve {
                points: vec![[0.5, 0.0], [0.5, 1.0]],
            },
            ..Curves::default()
        });
        assert!(bad_curve.validate().is_err());
        let bad_levels = Adjustment::Levels(Levels {
            master: LevelsChannel {
                input_black: 0.8,
                input_white: 0.2,
                ..LevelsChannel::default()
            },
            ..Levels::default()
        });
        assert!(bad_levels.validate().is_err());
        assert!(
            Adjustment::HueSaturation(HueSaturation {
                hue: 200.0,
                ..Default::default()
            })
            .validate()
            .is_err()
        );
        assert!(Filter::GaussianBlur { radius: 150.0 }.validate().is_err());
        assert!(
            Filter::UnsharpMask {
                radius: 2.0,
                amount: 1.5,
                threshold: 4.0
            }
            .validate()
            .is_ok()
        );
    }

    /// Largest gap between the linear interpolation of a lookup table and
    /// the curve, over dense samples, in 16-bit units.
    fn table_error(curve: &Curve) -> f64 {
        let table = curve.lookup_table();
        let values = &table.values;
        let (start, end) = (f64::from(table.start), f64::from(table.end));
        let last = (values.len() - 1) as f64;
        let samples = 400_000;
        (0..=samples)
            .map(|k| {
                // The input as the renderer holds it, in f32.
                let x32 = k as f32 / samples as f32;
                let x = f64::from(x32);
                // As the shader: the input is bounded to the domain.
                let u = (x.clamp(start, end) - start) / (end - start) * last;
                let i = (u.floor() as usize).min(values.len() - 2);
                let f = u - i as f64;
                let lerp =
                    f64::from(values[i]) + (f64::from(values[i + 1]) - f64::from(values[i])) * f;
                (lerp - f64::from(curve.evaluate(x32))).abs() * 65535.0
            })
            .fold(0.0, f64::max)
    }

    fn curve(points: &[[f32; 2]]) -> Curve {
        Curve {
            points: points.to_vec(),
        }
    }

    #[test]
    fn lookup_tables_stay_within_a_quarter_of_a_16_bit_unit() {
        // The curves of docs/decisions/0028, from gentle to very steep.
        for points in [
            &[[0.0, 0.0], [1.0, 1.0]][..],
            &[[0.0, 0.0], [0.25, 0.18], [0.75, 0.82], [1.0, 1.0]],
            &[[0.0, 0.0], [0.45, 0.05], [0.55, 0.95], [1.0, 1.0]],
            &[[0.0, 0.0], [0.9, 0.0], [1.0, 1.0]],
            &[[0.0, 0.0], [0.5, 0.02], [0.51, 0.98], [1.0, 1.0]],
            // Flat ends: the table covers 0.2..0.4 only, without the kinks.
            &[[0.2, 0.1], [0.4, 0.9]],
            &[[0.1, 0.0], [0.3, 0.7], [0.32, 0.75], [0.95, 1.0]],
        ] {
            let curve = curve(points);
            let error = table_error(&curve);
            // 0.25 of interpolation, plus the f32 rounding of `evaluate`
            // itself, which reaches 0.1 unit on the steepest curve (slope
            // about 140): the table cannot be closer than its reference.
            assert!(
                error <= 0.35,
                "{points:?}: {error} units with {} entries",
                curve.lookup_table().values.len()
            );
        }
    }

    #[test]
    fn lookup_tables_grow_only_for_steep_curves() {
        let size = |points: &[[f32; 2]]| curve(points).lookup_table().values.len();
        assert_eq!(size(&[[0.0, 0.0], [1.0, 1.0]]), MIN_CURVE_TABLE);
        assert_eq!(
            size(&[[0.0, 0.0], [0.25, 0.18], [0.75, 0.82], [1.0, 1.0]]),
            MIN_CURVE_TABLE
        );
        let steep = size(&[[0.0, 0.0], [0.5, 0.02], [0.51, 0.98], [1.0, 1.0]]);
        assert!(
            steep > MIN_CURVE_TABLE && steep <= MAX_CURVE_TABLE,
            "{steep}"
        );
        // Points closer than the table can follow: bounded size.
        assert_eq!(
            size(&[[0.0, 0.0], [0.5, 0.0], [0.5001, 1.0], [1.0, 1.0]]),
            MAX_CURVE_TABLE
        );
    }

    #[test]
    fn a_full_table_hits_every_16_bit_level_exactly() {
        let curve = curve(&[[0.0, 0.0], [0.5, 0.0], [0.5001, 1.0], [1.0, 1.0]]);
        let table = curve.lookup_table();
        assert_eq!((table.start, table.end), (0.0, 1.0));
        assert_eq!(table.values.len(), 65536);
        for level in (0..=65535u32).step_by(97) {
            let x = level as f32 / 65535.0;
            assert_eq!(
                table.values[level as usize],
                curve.evaluate(x),
                "level {level}"
            );
        }
    }
}

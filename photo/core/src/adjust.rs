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

/// Levels adjustment: the master channel, then red, green and blue.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize, JsonSchema)]
pub struct Levels {
    /// Applied to the three color channels.
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

    /// Value of the curve at `x` (0..=1). Flat outside the first and last
    /// points. This is the only definition of the curve shape.
    pub fn evaluate(&self, x: f32) -> f32 {
        let p = &self.points;
        let (Some(first), Some(last)) = (p.first(), p.last()) else {
            return x;
        };
        if p.len() < 2 || x <= first[0] {
            return first[1];
        }
        if x >= last[0] {
            return last[1];
        }
        let m = self.tangents();
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

    /// The curve sampled at `size` evenly spaced inputs from 0 to 1.
    pub fn table(&self, size: usize) -> Vec<f32> {
        let last = size.saturating_sub(1).max(1) as f32;
        (0..size).map(|i| self.evaluate(i as f32 / last)).collect()
    }
}

/// Curves adjustment (REG-3): master curve, then per channel.
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
}

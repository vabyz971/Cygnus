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

//! Hue / Saturation / Lightness (O005): hue rotation in degrees,
//! saturation and lightness in -100..=100 (0 = neutral), per pixel in
//! HSL space. Alpha untouched; neutral settings return the input as is.

use super::{Effect, NodeCtx, to_rgba8};
use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};
use image::DynamicImage;
use rayon::prelude::*;

/// Registry id.
pub const TYPE_ID: &str = "hsl";

pub fn definition() -> NodeDefinition {
    NodeDefinition::new(TYPE_ID, "Hue / Saturation / Lightness", NodeCategory::Color)
        .input(SocketDef::new("image", "Image", SocketType::Image))
        .output(SocketDef::new("image", "Image", SocketType::Image))
        .param("hue", ParamValue::Float(0.0))
        .param("saturation", ParamValue::Float(0.0))
        .param("lightness", ParamValue::Float(0.0))
        .header_color([0.80, 0.40, 0.55])
        .description("Hue rotation, saturation and lightness")
}

/// HSL settings (`hue` in degrees, the others in -100..=100).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HslShift {
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}

impl HslShift {
    fn is_neutral(&self) -> bool {
        self.hue.abs() < 0.01 && self.saturation.abs() < 0.01 && self.lightness.abs() < 0.01
    }

    /// Transforms one RGB triple (0..=1 components).
    #[must_use]
    pub fn apply_rgb(&self, rgb: [f32; 3]) -> [f32; 3] {
        let [mut h, mut s, mut l] = rgb_to_hsl(rgb);
        h = (h + self.hue / 360.0).rem_euclid(1.0);
        let ks = (self.saturation / 100.0).clamp(-1.0, 1.0);
        s = if ks < 0.0 {
            s * (1.0 + ks)
        } else {
            s + (1.0 - s) * ks * s
        };
        let kl = (self.lightness / 100.0).clamp(-1.0, 1.0);
        l = if kl < 0.0 {
            l * (1.0 + kl)
        } else {
            l + (1.0 - l) * kl
        };
        hsl_to_rgb([h, s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)])
    }
}

/// RGB (0..=1) → HSL (h in 0..1).
#[must_use]
pub fn rgb_to_hsl([r, g, b]: [f32; 3]) -> [f32; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return [0.0, 0.0, l];
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if (max - r).abs() < 1e-6 {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if (max - g).abs() < 1e-6 {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    };
    [h / 6.0, s, l]
}

/// HSL (h in 0..1) → RGB (0..=1).
#[must_use]
pub fn hsl_to_rgb([h, s, l]: [f32; 3]) -> [f32; 3] {
    if s < 1e-6 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    let channel = |mut t: f32| {
        t = t.rem_euclid(1.0);
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 0.5 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    };
    [channel(h + 1.0 / 3.0), channel(h), channel(h - 1.0 / 3.0)]
}

/// Applies the shift to the whole image (rayon, per pixel).
#[must_use]
pub fn apply_effect(img: &DynamicImage, shift: HslShift) -> DynamicImage {
    if shift.is_neutral() {
        return img.clone();
    }
    let mut out = to_rgba8(img);
    out.as_flat_samples_mut()
        .samples
        .par_chunks_mut(4)
        .for_each(|px| {
            let rgb = [
                f32::from(px[0]) / 255.0,
                f32::from(px[1]) / 255.0,
                f32::from(px[2]) / 255.0,
            ];
            let [r, g, b] = shift.apply_rgb(rgb);
            px[0] = (r * 255.0).round().clamp(0.0, 255.0) as u8;
            px[1] = (g * 255.0).round().clamp(0.0, 255.0) as u8;
            px[2] = (b * 255.0).round().clamp(0.0, 255.0) as u8;
        });
    DynamicImage::ImageRgba8(out)
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    let input = ctx.input()?;
    let shift = HslShift {
        hue: ctx.param("hue", 0.0),
        saturation: ctx.param("saturation", 0.0),
        lightness: ctx.param("lightness", 0.0),
    };
    Some(apply_effect(input, shift))
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

    const NEUTRAL: HslShift = HslShift {
        hue: 0.0,
        saturation: 0.0,
        lightness: 0.0,
    };

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b.iter()).all(|(x, y)| (x - y).abs() < 1e-3)
    }

    #[test]
    fn hsl_round_trip() {
        for rgb in [
            [1.0, 0.0, 0.0],
            [0.2, 0.6, 0.4],
            [0.9, 0.9, 0.1],
            [0.5, 0.5, 0.5],
        ] {
            assert!(close(hsl_to_rgb(rgb_to_hsl(rgb)), rgb), "{rgb:?}");
        }
    }

    #[test]
    fn neutral_is_identity() {
        let rgb = [0.3, 0.7, 0.2];
        assert!(close(NEUTRAL.apply_rgb(rgb), rgb));
    }

    #[test]
    fn hue_120_turns_red_into_green() {
        let out = HslShift {
            hue: 120.0,
            ..NEUTRAL
        }
        .apply_rgb([1.0, 0.0, 0.0]);
        assert!(close(out, [0.0, 1.0, 0.0]), "{out:?}");
    }

    #[test]
    fn full_desaturation_gives_gray() {
        let out = HslShift {
            saturation: -100.0,
            ..NEUTRAL
        }
        .apply_rgb([0.8, 0.2, 0.4]);
        assert!((out[0] - out[1]).abs() < 1e-3 && (out[1] - out[2]).abs() < 1e-3);
    }

    #[test]
    fn lightness_extremes() {
        let white = HslShift {
            lightness: 100.0,
            ..NEUTRAL
        }
        .apply_rgb([0.3, 0.4, 0.5]);
        assert!(close(white, [1.0, 1.0, 1.0]));
        let black = HslShift {
            lightness: -100.0,
            ..NEUTRAL
        }
        .apply_rgb([0.3, 0.4, 0.5]);
        assert!(close(black, [0.0, 0.0, 0.0]));
    }

    #[test]
    fn alpha_is_preserved() {
        let img = DynamicImage::ImageRgba8(image::ImageBuffer::from_pixel(
            1,
            1,
            image::Rgba([200, 50, 50, 77]),
        ));
        let out = apply_effect(
            &img,
            HslShift {
                hue: 90.0,
                ..NEUTRAL
            },
        )
        .to_rgba8();
        assert_eq!(out.get_pixel(0, 0)[3], 77);
    }
}

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

//! Levels (O005): input black/white points, midtone gamma and output
//! range, applied to R, G and B through a single 256-entry LUT (alpha
//! untouched). Neutral defaults = identity, zero copy skipped by the
//! LUT check.

use super::{Effect, NodeCtx, to_rgba8};
use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};
use image::DynamicImage;
use rayon::prelude::*;

/// Registry id.
pub const TYPE_ID: &str = "levels";

pub fn definition() -> NodeDefinition {
    NodeDefinition::new(TYPE_ID, "Niveaux", NodeCategory::Color)
        .input(SocketDef::new("image", "Image", SocketType::Image))
        .output(SocketDef::new("image", "Image", SocketType::Image))
        .param("in_black", ParamValue::Float(0.0))
        .param("in_white", ParamValue::Float(255.0))
        .param("gamma", ParamValue::Float(1.0))
        .param("out_black", ParamValue::Float(0.0))
        .param("out_white", ParamValue::Float(255.0))
        .header_color([0.55, 0.55, 0.60])
        .description("Points noir/blanc, gamma et plage de sortie")
}

/// Levels settings (8-bit scale, gamma > 0).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Levels {
    pub in_black: f32,
    pub in_white: f32,
    pub gamma: f32,
    pub out_black: f32,
    pub out_white: f32,
}

impl Levels {
    /// Builds the LUT. Degenerate input range (white <= black) is
    /// widened to one step so the division stays finite.
    #[must_use]
    pub fn lut(&self) -> [u8; 256] {
        let in_black = self.in_black.clamp(0.0, 254.0);
        let in_white = self.in_white.clamp(in_black + 1.0, 255.0);
        let gamma = self.gamma.clamp(0.1, 10.0);
        let out_black = self.out_black.clamp(0.0, 255.0);
        let out_white = self.out_white.clamp(0.0, 255.0);
        let mut lut = [0u8; 256];
        for (i, slot) in lut.iter_mut().enumerate() {
            let t = ((i as f32 - in_black) / (in_white - in_black)).clamp(0.0, 1.0);
            let t = t.powf(1.0 / gamma);
            *slot = (out_black + t * (out_white - out_black))
                .round()
                .clamp(0.0, 255.0) as u8;
        }
        lut
    }
}

/// True when the LUT maps every value to itself (no work to do).
#[must_use]
pub(crate) fn is_identity(lut: &[u8; 256]) -> bool {
    lut.iter().enumerate().all(|(i, v)| usize::from(*v) == i)
}

/// Applies the same LUT to R, G and B (alpha untouched).
#[must_use]
pub(crate) fn apply_rgb_lut(img: &DynamicImage, lut: &[u8; 256]) -> DynamicImage {
    if is_identity(lut) {
        return img.clone();
    }
    let mut out = to_rgba8(img);
    out.as_flat_samples_mut()
        .samples
        .par_chunks_mut(4)
        .for_each(|px| {
            px[0] = lut[usize::from(px[0])];
            px[1] = lut[usize::from(px[1])];
            px[2] = lut[usize::from(px[2])];
        });
    DynamicImage::ImageRgba8(out)
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    let input = ctx.input()?;
    let levels = Levels {
        in_black: ctx.param("in_black", 0.0),
        in_white: ctx.param("in_white", 255.0),
        gamma: ctx.param("gamma", 1.0),
        out_black: ctx.param("out_black", 0.0),
        out_white: ctx.param("out_white", 255.0),
    };
    Some(apply_rgb_lut(input, &levels.lut()))
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

    const NEUTRAL: Levels = Levels {
        in_black: 0.0,
        in_white: 255.0,
        gamma: 1.0,
        out_black: 0.0,
        out_white: 255.0,
    };

    #[test]
    fn neutral_levels_are_identity() {
        assert!(is_identity(&NEUTRAL.lut()));
    }

    #[test]
    fn input_points_stretch_the_range() {
        let lut = Levels {
            in_black: 50.0,
            in_white: 200.0,
            ..NEUTRAL
        }
        .lut();
        assert_eq!(lut[0], 0);
        assert_eq!(lut[50], 0);
        assert_eq!(lut[200], 255);
        assert_eq!(lut[255], 255);
        // Midpoint of the input range lands mid-scale
        assert!((i16::from(lut[125]) - 128).abs() <= 1);
    }

    #[test]
    fn gamma_above_one_brightens_midtones() {
        let lut = Levels {
            gamma: 2.0,
            ..NEUTRAL
        }
        .lut();
        assert!(lut[128] > 170);
        assert_eq!(lut[0], 0);
        assert_eq!(lut[255], 255);
    }

    #[test]
    fn output_range_compresses() {
        let lut = Levels {
            out_black: 20.0,
            out_white: 220.0,
            ..NEUTRAL
        }
        .lut();
        assert_eq!(lut[0], 20);
        assert_eq!(lut[255], 220);
    }

    #[test]
    fn degenerate_input_range_stays_finite() {
        let lut = Levels {
            in_black: 200.0,
            in_white: 100.0,
            ..NEUTRAL
        }
        .lut();
        assert_eq!(lut[0], 0);
        assert_eq!(lut[255], 255);
    }
}

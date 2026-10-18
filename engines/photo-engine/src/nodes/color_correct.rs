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

//! Color Correction effect (simplified HSL saturation)

use super::{Effect, NodeCtx, to_rgba8};
use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};
use image::DynamicImage;
use rayon::prelude::*;

pub fn definition() -> NodeDefinition {
    NodeDefinition::new("color_correct", "Color correction", NodeCategory::Color)
        .input(SocketDef::new("image", "Image", SocketType::Image))
        .output(SocketDef::new("image", "Image", SocketType::Image))
        .param("saturation", ParamValue::Float(1.0))
        .param("hue", ParamValue::Float(0.0))
        .header_color([0.85, 0.55, 0.10])
        .description("HSL correction")
}

pub fn apply_effect(img: &DynamicImage, saturation: f32) -> DynamicImage {
    if let Some(gpu_out) = super::apply_saturation_gpu(img, saturation) {
        return gpu_out;
    }
    let s = saturation.clamp(0.0, 3.0);
    if (s - 1.0).abs() < 0.01 {
        return img.clone();
    }
    let mut out = to_rgba8(img);
    out.as_flat_samples_mut()
        .samples
        .par_chunks_mut(4)
        .for_each(|px| {
            let r = px[0] as f32;
            let g = px[1] as f32;
            let b = px[2] as f32;
            let gray = 0.299 * r + 0.587 * g + 0.114 * b;
            px[0] = (gray + (r - gray) * s).clamp(0.0f32, 255.0f32) as u8;
            px[1] = (gray + (g - gray) * s).clamp(0.0f32, 255.0f32) as u8;
            px[2] = (gray + (b - gray) * s).clamp(0.0f32, 255.0f32) as u8;
        });
    DynamicImage::ImageRgba8(out)
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    let input = ctx.input()?;
    let s = ctx.param("saturation", 1.0);
    Some(apply_effect(input, s))
}

pub fn effect() -> Effect {
    Effect {
        definition: definition(),
        apply,
    }
}

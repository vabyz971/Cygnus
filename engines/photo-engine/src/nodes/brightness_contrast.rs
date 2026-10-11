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

//! Brightness / Contrast effect (GPU compute + rayon CPU fallback)

use super::{Effect, NodeCtx, to_rgba8};
use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};
use image::DynamicImage;
use rayon::prelude::*;

pub fn definition() -> NodeDefinition {
    NodeDefinition::new(
        "brightness_contrast",
        "Luminosité / Contraste",
        NodeCategory::Color,
    )
    .input(SocketDef::new("image", "Image", SocketType::Image))
    .output(SocketDef::new("image", "Image", SocketType::Image))
    .param("brightness", ParamValue::Float(0.0))
    .param("contrast", ParamValue::Float(0.0))
    .header_color([0.75, 0.55, 0.15])
    .description("Ajuste luminosité et contraste")
}

pub fn apply_effect(img: &DynamicImage, brightness: f32, contrast: f32) -> DynamicImage {
    if let Some(gpu_out) = super::apply_brightness_contrast_gpu(img, brightness, contrast) {
        return gpu_out;
    }
    let b = (brightness * 2.55) as i32;
    let contrast_factor = if contrast < 0.0 {
        1.0 + contrast / 100.0
    } else {
        1.0 + contrast / 50.0
    };
    let mut out = to_rgba8(img);
    out.as_flat_samples_mut()
        .samples
        .par_chunks_mut(4)
        .for_each(|px| {
            for c in px.iter_mut().take(3) {
                let mut v = *c as f32;
                v = (v - 128.0) * contrast_factor + 128.0;
                v += b as f32;
                *c = v.clamp(0.0f32, 255.0f32) as u8;
            }
        });
    DynamicImage::ImageRgba8(out)
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    let input = ctx.input()?;
    let b = ctx.param("brightness", 0.0);
    let c = ctx.param("contrast", 0.0);
    Some(apply_effect(input, b, c))
}

pub fn effect() -> Effect {
    Effect {
        definition: definition(),
        apply,
    }
}

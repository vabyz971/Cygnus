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

//! Gaussian Blur effect (GPU compute + CPU `image::blur` fallback)

use super::{Effect, NodeCtx};
use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};
use image::DynamicImage;

pub fn definition() -> NodeDefinition {
    NodeDefinition::new("blur", "Flou", NodeCategory::Filter)
        .input(SocketDef::new("image", "Image", SocketType::Image))
        .output(SocketDef::new("image", "Image", SocketType::Image))
        .param("radius", ParamValue::Float(5.0))
        .param("type", ParamValue::Enum("Gaussian".into()))
        .header_color([0.20, 0.55, 0.75])
        .description("Flou gaussien")
}

pub fn apply_effect(img: &DynamicImage, radius: f32) -> DynamicImage {
    if radius <= 0.1 {
        return img.clone();
    }
    if let Some(gpu_out) = super::apply_blur_gpu(img, radius) {
        return gpu_out;
    }
    img.blur(radius)
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    let input = ctx.input()?;
    let r = ctx.param("radius", 5.0);
    Some(apply_effect(input, r))
}

pub fn effect() -> Effect {
    Effect {
        definition: definition(),
        apply,
    }
}

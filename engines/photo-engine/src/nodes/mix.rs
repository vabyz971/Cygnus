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

//! Mix node: stacks 2 to 6 images (dynamic inputs) in slot order
//! — image 1 at the bottom, the last on top. Each unconnected
//! input is ignored. It is the tool for stacking more than two layers.
//!
//! Reuses the Layer blend (modes + alpha compositing).

use super::layer::{MIX_MAX_INPUTS, mix_socket};
use super::{Effect, NodeCtx};
use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};
use image::DynamicImage;

pub fn definition() -> NodeDefinition {
    let mut def = NodeDefinition::new("mix", "Mélange", NodeCategory::Compositing)
        .param("count", ParamValue::Int(2))
        .param("blend_mode", ParamValue::Enum("Normal".into()))
        .header_color([0.45, 0.35, 0.65])
        .description("Superpose 2 à 6 images (ajouter/retirer des entrées)");
    for i in 1..=MIX_MAX_INPUTS {
        def = def.input(SocketDef::new(
            mix_socket(i),
            format!("Image {i}"),
            SocketType::Image,
        ));
    }
    def.output(SocketDef::new("image", "Image", SocketType::Image))
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    // Linear chain = single image: nothing to stack, forward it
    // as is (multi-input stacking only exists in graph mode).
    ctx.input().cloned()
}

pub fn effect() -> Effect {
    Effect {
        definition: definition(),
        apply,
    }
}

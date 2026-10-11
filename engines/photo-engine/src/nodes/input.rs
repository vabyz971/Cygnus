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

//! Source node: feeds the original image into the chain

use super::{Effect, NodeCtx};
use datatypes::{NodeCategory, NodeDefinition, SocketDef, SocketType};
use image::DynamicImage;

pub fn definition() -> NodeDefinition {
    NodeDefinition::new("input_image", "Image Source", NodeCategory::Input)
        .output(SocketDef::new("image", "Image", SocketType::Image))
        .header_color([0.25, 0.45, 0.75])
        .description("Source d'image")
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    // Chain head: forwards the input if any, otherwise the original.
    ctx.input().cloned().or_else(|| Some(ctx.original.clone()))
}

pub fn effect() -> Effect {
    Effect {
        definition: definition(),
        apply,
    }
}

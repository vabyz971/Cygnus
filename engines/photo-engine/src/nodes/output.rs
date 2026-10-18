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

//! Output node: forwards the final image to the canvas

use super::{Effect, NodeCtx};
use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};
use image::DynamicImage;

pub fn definition() -> NodeDefinition {
    NodeDefinition::new("output", "Output", NodeCategory::Output)
        .input(SocketDef::new("image", "Image", SocketType::Image))
        .param("width", ParamValue::Int(1920))
        .param("height", ParamValue::Int(1080))
        .header_color([0.65, 0.20, 0.20])
        .description("Final output, cropped to the gizmo (chosen size)")
}

fn apply(ctx: &NodeCtx) -> Option<DynamicImage> {
    // For display, the output forwards the image as is;
    // the dimensions (gizmo) are only used for export and the visual overlay.
    // Export cropping happens at export time, not here.
    ctx.input().cloned()
}

pub fn effect() -> Effect {
    Effect {
        definition: definition(),
        apply,
    }
}

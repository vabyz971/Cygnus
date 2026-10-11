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

//! Photo effect nodes — one file per effect.
//! Each effect exposes:
//!  - `definition()`: its definition (sockets + params) for the UI registry
//!  - `apply(ctx)`: its image -> image evaluation in a linear chain
//!    (see [`crate::filters`]).
//!
//! Adding a new effect = create a file here + register it in `all()`.

pub mod blur;
pub mod brightness_contrast;
pub mod color_correct;
pub mod input;
pub mod layer;
pub mod mix;
pub mod output;

use crate::gpu;
use datatypes::{NodeDefinition, ParamValue};
use image::DynamicImage;
use std::collections::HashMap;

/// Context passed to each effect when evaluating a linear filter chain
/// : the current node parameters + the image produced by
/// the previous step. No graph structure — `filters.rs` folds the
/// chain sequentially.
pub struct NodeCtx<'a> {
    /// Current node parameters (e.g. `{"brightness": Float(50.0)}`)
    pub params: &'a HashMap<String, ParamValue>,
    /// Image produced by the previous chain step
    pub input_image: Option<&'a DynamicImage>,
    /// Original source image (fallback when there is no input)
    pub original: &'a DynamicImage,
}

impl NodeCtx<'_> {
    /// Image arriving from the previous chain step
    pub fn input(&self) -> Option<&DynamicImage> {
        self.input_image
    }

    /// Float value of a parameter, with default
    pub fn param(&self, key: &str, default: f32) -> f32 {
        self.params
            .get(key)
            .and_then(|v| v.as_float())
            .unwrap_or(default)
    }
}

/// A registered effect: UI definition + evaluation function
pub struct Effect {
    pub definition: NodeDefinition,
    pub apply: fn(&NodeCtx) -> Option<DynamicImage>,
}

/// All engine effects — single entry point for registry and processor.
pub fn all() -> Vec<Effect> {
    vec![
        input::effect(),
        output::effect(),
        brightness_contrast::effect(),
        blur::effect(),
        mix::effect(),
        layer::effect(),
        color_correct::effect(),
    ]
}

/// Finds an effect by type_id
pub fn find(type_id: &str) -> Option<Effect> {
    all().into_iter().find(|e| e.definition.type_id == type_id)
}

// ---------------------------------------------------------------------------
// Helpers shared by CPU/GPU effects
// ---------------------------------------------------------------------------

/// Converts a DynamicImage to raw RGBA8
pub fn to_rgba8(img: &DynamicImage) -> image::ImageBuffer<image::Rgba<u8>, Vec<u8>> {
    img.to_rgba8()
}

pub use gpu::{apply_blur_gpu, apply_brightness_contrast_gpu, apply_mix_gpu, apply_saturation_gpu};

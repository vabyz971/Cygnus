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

//! Layout engine: frames, constraints, pages, placement.
//!
//! This engine owns the layout BUSINESS LOGIC (boxes, constraints,
//! row/column stacks, alignment, pagination) — never drawing:
//! no `wgpu`, no `egui`, no renderer. The result feeds the
//! Scene Graph ([`apply_to_scene`]); text measurement is delegated to the
//! [`TextMeasurer`] trait (shaping lives in `text-engine`, never here).

pub mod layout;
pub mod measure;
pub mod model;
pub mod scene_bridge;

pub use layout::{ComputedLayout, layout};
pub use measure::{NullMeasurer, TextMeasurer};
pub use model::{
    Alignment, Constraints, Content, Direction, Frame, FrameId, FrameKind, Insets, LayoutDoc,
};
pub use scene_bridge::apply_to_scene;

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

//! Vector engine: Bézier paths, shapes, fills, strokes.
//!
//! This engine owns the vector BUSINESS LOGIC (geometry, styles,
//! boolean operations as data) — never drawing: no `wgpu`,
//! no `egui`, no Vello dependency. Rendering goes through the
//! [`VectorBackend`] trait (a future Vello adapter will implement it with its
//! own types, without this model knowing Vello).
//!
//! Reused shared foundations: `datatypes::Vec2`/`Rect` (geometry),
//! `math-utils::Transform2D` (placement), `ids::EntityId` (identities).

pub mod backend;
pub mod document;
pub mod geometry;
pub mod style;

pub use backend::{NullBackend, VectorBackend, render_scene};
pub use document::{Shape, ShapeGeometry, ShapeId, VectorScene};
pub use geometry::{Path, PathVerb};
pub use ids::EntityId;
pub use style::{
    BooleanOp, Color, Fill, GradientStop, LineCap, LineJoin, LinearGradient, RadialGradient, Stroke,
};

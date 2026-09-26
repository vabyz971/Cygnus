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

//! Moteur vectoriel : paths Bézier, formes, remplissages, contours.
//!
//! Ce moteur possède la LOGIQUE MÉTIER vectorielle (géométrie, styles,
//! opérations booléennes comme données) — jamais le dessin : aucun `wgpu`,
//! aucun `egui`, aucune dépendance Vello. Le rendu passe par le trait
//! [`VectorBackend`] (un futur adaptateur Vello l'implémentera avec ses
//! propres types, sans que ce modèle ne connaisse Vello).
//!
//! Fondations partagées réutilisées : `datatypes::Vec2`/`Rect` (géométrie),
//! `math-utils::Transform2D` (placement), `ids::EntityId` (identités).

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

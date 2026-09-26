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

//! Moteur de mise en page : cadres, contraintes, pages, placement.
//!
//! Ce moteur possède la LOGIQUE MÉTIER layout (boîtes, contraintes,
//! empilements ligne/colonne, alignement, pagination) — jamais le dessin :
//! aucun `wgpu`, aucun `egui`, aucun renderer. Le résultat alimente le
//! Scene Graph ([`apply_to_scene`]) ; la mesure du texte est déléguée au
//! trait [`TextMeasurer`] (le façonnage vit dans `text-engine`, jamais ici).

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

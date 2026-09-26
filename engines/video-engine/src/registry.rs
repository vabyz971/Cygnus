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

//! Registre nodal vidéo LEGACY (bibliothèque de nœuds UI).
//!
//! Conservé tel quel : il alimente l'éditeur nodal générique (`datatypes`)
//! pendant que le modèle éditorial ([`Timeline`](crate::Timeline)) grandit
//! à côté. Les deux coexistent — aucune suppression, migration douce.

use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};

/// Définitions nodales vidéo (inchangé — voir `timeline` pour le modèle).
#[must_use]
pub fn all_definitions() -> Vec<NodeDefinition> {
    vec![
        NodeDefinition::new("clip_input", "Clip Source", NodeCategory::Input)
            .output(SocketDef::new("video", "Video", SocketType::Image))
            .output(SocketDef::new("audio", "Audio", SocketType::Vector))
            .header_color([0.25, 0.45, 0.75])
            .description("Source clip vidéo"),
        NodeDefinition::new("transition", "Transition", NodeCategory::Compositing)
            .input(SocketDef::new("video_a", "Video A", SocketType::Image))
            .input(SocketDef::new("video_b", "Video B", SocketType::Image))
            .input(SocketDef::new("progress", "Progress", SocketType::Float))
            .output(SocketDef::new("video", "Video", SocketType::Image))
            .param("type", ParamValue::Enum("CrossDissolve".into()))
            .param("progress", ParamValue::Float(0.5))
            .header_color([0.45, 0.35, 0.65])
            .description("Transition Final Cut"),
        NodeDefinition::new("color_grade", "Étalonnage", NodeCategory::Color)
            .input(SocketDef::new("video", "Video", SocketType::Image))
            .output(SocketDef::new("video", "Video", SocketType::Image))
            .param("exposure", ParamValue::Float(0.0))
            .header_color([0.75, 0.55, 0.15])
            .description("Étalonnage couleur"),
        NodeDefinition::new("output_video", "Sortie Vidéo", NodeCategory::Output)
            .input(SocketDef::new("video", "Video", SocketType::Image))
            .header_color([0.65, 0.20, 0.20])
            .description("Master timeline"),
    ]
}

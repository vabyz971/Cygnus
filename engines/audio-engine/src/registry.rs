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

//! Registre nodal audio LEGACY (bibliothèque de nœuds UI).
//!
//! Conservé tel quel : il alimente l'éditeur nodal générique (`datatypes`)
//! pendant que le graphe DSP ([`AudioGraph`](crate::AudioGraph)) grandit à
//! côté. Les deux coexistent — aucune suppression, migration douce.

use datatypes::{NodeCategory, NodeDefinition, ParamValue, SocketDef, SocketType};

/// Définitions nodales audio (inchangé — voir `node`/`graph` pour le DSP).
#[must_use]
pub fn all_definitions() -> Vec<NodeDefinition> {
    vec![
        NodeDefinition::new("sample_input", "Sample", NodeCategory::Input)
            .output(SocketDef::new("audio", "Audio", SocketType::Vector))
            .header_color([0.20, 0.55, 0.35])
            .description("Sample audio"),
        NodeDefinition::new("oscillator", "Oscillateur", NodeCategory::Utility)
            .output(SocketDef::new("audio", "Audio", SocketType::Vector))
            .param("freq", ParamValue::Float(440.0))
            .param("wave", ParamValue::Enum("Sine".into()))
            .header_color([0.85, 0.55, 0.10])
            .description("FL Studio oscillator"),
        NodeDefinition::new("filter", "Filtre", NodeCategory::Filter)
            .input(SocketDef::new("audio", "Audio", SocketType::Vector))
            .output(SocketDef::new("audio", "Audio", SocketType::Vector))
            .param("cutoff", ParamValue::Float(1000.0))
            .param("res", ParamValue::Float(0.5))
            .header_color([0.20, 0.55, 0.75])
            .description("Filtre passe-bas"),
        NodeDefinition::new("mixer", "Mixer", NodeCategory::Compositing)
            .input(SocketDef::new("audio_a", "Audio A", SocketType::Vector))
            .input(SocketDef::new("audio_b", "Audio B", SocketType::Vector))
            .input(SocketDef::new("gain", "Gain", SocketType::Float))
            .output(SocketDef::new("audio", "Audio", SocketType::Vector))
            .param("gain", ParamValue::Float(0.8))
            .header_color([0.45, 0.35, 0.65])
            .description("Table de mixage"),
        NodeDefinition::new("output_audio", "Master Out", NodeCategory::Output)
            .input(SocketDef::new("audio", "Audio", SocketType::Vector))
            .header_color([0.65, 0.20, 0.20])
            .description("Sortie master"),
    ]
}

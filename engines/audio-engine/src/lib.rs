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

//! Moteur audio : timeline, graphe DSP, effets — HORS Scene Graph.
//!
//! L'audio n'est PAS spatial : aucun `Scene`, aucun `Transform2D`, aucun
//! pixel. Il partage avec les autres domaines les fondations génériques
//! (`ids`, `graph` : dépendances + cycles + topo, `Revision`, sérialisation)
//! et garde ses propres nœuds métier :
//!
//! ```text
//! Audio Timeline → Audio Graph → DSP Backend
//!   (régions)      (Source/Gain/EQ/Compressor/Reverb/Mixer)
//! ```

pub mod dsp;
pub mod graph;
pub mod node;
pub mod registry;
pub mod timeline;

pub use dsp::{DspBackend, NullBackend, ProcessReport, SampleBuffer, gain_linear, mix_into};
pub use graph::AudioGraph;
pub use ids::EntityId;
pub use node::{AudioId, AudioNode, AudioNodeKind, SourceKind, Wave};
pub use registry::all_definitions;
pub use timeline::{AudioTimeline, Region};

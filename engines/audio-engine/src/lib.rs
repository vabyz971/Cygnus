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

//! Audio engine: timeline, DSP graph, effects — OUTSIDE the Scene Graph.
//!
//! Audio is NOT spatial: no `Scene`, no `Transform2D`, no
//! pixels. It shares the generic foundations with the other domains
//! (`ids`, `graph`: dependencies + cycles + topo, `Revision`, serialization)
//! and keeps its own business nodes:
//!
//! ```text
//! Audio Timeline → Audio Graph → DSP Backend
//!   (regions)      (Source/Gain/EQ/Compressor/Reverb/Mixer)
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

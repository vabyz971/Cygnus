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

//! Moteur vidéo : clips, timeline éditoriale, transitions, décodage.
//!
//! Pipeline visé (pas de lecteur complet ici) :
//!
//! ```text
//! Video Clip → Decoder → Frame → Raster pipeline → Effects → Composite
//! ```
//!
//! Ce moteur possède la LOGIQUE ÉDITORIALE (clips, timeline, transitions)
//! et décrit le compositing (`CompositeDesc` avec le [`BlendMode`] PARTAGÉ
//! de `datatypes` — mêmes modes que la photo, exécutés plus tard par le
//! backend raster commun). Aucun pixel, aucun `wgpu` : les décodeurs réels
//! implémenteront [`FrameDecoder`] hors de ce crate.

pub mod clip;
pub mod composite;
pub mod decoder;
pub mod registry;
pub mod timeline;

pub use clip::{Clip, ClipId, FrameDescriptor};
pub use composite::{CompositeDesc, Transition, TransitionKind};
pub use datatypes::BlendMode;
pub use decoder::{FrameDecoder, NullDecoder};
pub use ids::EntityId;
pub use registry::all_definitions;
pub use timeline::{ActiveClip, Timeline, TimelineClip, Track};

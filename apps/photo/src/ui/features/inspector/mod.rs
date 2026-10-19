// Cygnus — Professional open-source creative suite
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

//! Inspector feature: properties of the selected layer.
//!
//! Panel (`panel`), per-domain sections (`sections/`) and
//! conversion to [`PhotoAction`](crate::commands::PhotoAction)
//! (`actions`). Next sections: `transform`, `blend`,
//! `effects` (same contract as `appearance`).

pub mod actions;
pub mod panel;
pub mod sections;

pub use actions::inspector_action_to_photo;
pub use panel::{InspectorAction, InspectorPanel};

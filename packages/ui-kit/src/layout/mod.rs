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

//! Workspace : état du layout partagé par les 3 apps.
//!
//! [`PanelId`] stables, [`PanelState`] (visibilité + dimension),
//! [`WorkspaceState`] (régions gauche / droite / basse + vue
//! centrale) et [`persistence`] JSON (restauration via
//! `packages/preferences`). Pas de docking dans cette phase.

pub mod panel_id;
pub mod panel_state;
pub mod persistence;
pub mod workspace_state;

pub use panel_id::PanelId;
pub use panel_state::{DEFAULT_PANEL_SIZE, PanelState};
pub use persistence::{load_workspace, save_workspace};
pub use workspace_state::{WORKSPACE_VERSION, WorkspaceState};

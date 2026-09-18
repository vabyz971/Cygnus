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

//! Workspace Photo : composition globale, sans logique de panels.
//!
//! ```text
//! PhotoWorkspace
//!   ↓ Top / Central (Tree) / Bottom (+ overlays)
//! Tile / Feature (contenu métier)
//!   ↓
//! ui-kit (style + conteneurs génériques)
//! ```
//!
//! Chaque région dessine son `egui::Panel` et délègue le contenu aux
//! features ; les actions remontent en [`PhotoAction`](crate::commands::PhotoAction).
//! Le workspace ne connaît ni calques, ni outils, ni moteurs.
//! Les quatre panneaux métier (outils, canevas, inspecteur, calques)
//! sont des tuiles `egui_tiles` (voir [`dock`]).

pub mod bottom_bar;
pub mod central_view;
pub mod dock;
pub mod left_sidebar;
pub mod overlays;
pub mod right_sidebar;
pub mod top_bar;
pub mod welcome;
pub mod workspace;

pub use workspace::PhotoWorkspace;

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

//! History feature: position-independent panel.
//!
//! Undo step list (oldest → newest), current state
//! highlighted, redo steps (next one first). Clicking a row =
//! state jump ([`HistoryPanelAction`]), converted to
//! [`PhotoAction`](crate::commands::PhotoAction) (see
//! [`super::actions`]). ui-kit-only styling, no hardcoded
//! color.

pub mod actions;
pub mod panel;

pub use actions::history_panel_action_to_photo;
pub use panel::HistoryPanel;

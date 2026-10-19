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

//! Photo workspace: global composition, no panel logic.
//!
//! ```text
//! PhotoWorkspace
//!   ↓ Top / Central (Tree) / Bottom (+ overlays)
//! Tile / Feature (domain content)
//!   ↓
//! cygnus-ui (style + generic containers)
//! ```
//!
//! Each region draws its `egui::Panel` and delegates content to the
//! features; actions bubble up as [`PhotoAction`](crate::commands::PhotoAction).
//! The workspace knows neither layers, nor tools, nor engines.
//! The four domain panels (tools, canvas, inspector, layers)
//! are `egui_tiles` tiles (see [`dock`]).

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

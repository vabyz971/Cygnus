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

//! UI persistence: document vs workspace.
//!
//! The user layout is saved INDEPENDENTLY of the document
//! (`.cygp` project engine-side). `ui_state` serializes the
//! [`WorkspaceState`](ui_kit::layout::WorkspaceState) to JSON via
//! ui-kit; the disk wiring (paths, `preferences`) will come
//! with the disk hookup — hence the pending functions.

pub mod ui_state;

pub use ui_state::{load_tree_or_default, load_workspace_or_default};

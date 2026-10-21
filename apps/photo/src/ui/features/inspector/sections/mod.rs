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

//! Inspector sections (one per layer domain).
//!
//! Today: `attachment` (focused attachment from
//! the layer tree: name, type, on/off). Future sections
//! (`transform`, `blend`, `effects`) will follow the same contract:
//! `(ui, ctx, …) -> Vec<InspectorAction>`.
//!
//! Visibility / opacity / blend live in the Layers panel
//! (selection header): never duplicated here.

pub mod attachment;
pub mod curve_editor;
pub mod styles;

pub use attachment::draw_attachment_section;
pub use styles::draw_styles_section;

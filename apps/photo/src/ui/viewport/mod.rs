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

//! Photo viewport: composition around the generic viewport.
//!
//! Separate layers, bottom to top:
//!
//! - [`canvas`]: `PhotoCanvas` (engine texture + tools, pan/zoom
//!   state-only), with no worker send;
//! - [`overlays`]: photo marker (origin) via the
//!   agnostic ui-kit helpers.
//!
//! The generic camera lives in ui-kit; photo rendering and selection
//! stay here, with no engine domain logic.

pub mod canvas;
pub mod overlays;

pub use canvas::{
    CanvasMapping, PaintRequest, PhotoBrushSettings, PhotoCanvas, PhotoCanvasOutcome,
    PhotoCanvasTool,
};
pub use overlays::{draw_brush_cursor, draw_document_bounds};

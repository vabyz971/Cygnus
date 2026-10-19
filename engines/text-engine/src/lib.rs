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

//! Text engine: model → layout → glyph runs → (render graph).
//!
//! Conceptual separation (no real shaping without fonts — metrics
//! come from the [`FontProvider`] trait, implemented for real later):
//!
//! ```text
//! TextModel (styled spans) ──layout()──▶ LaidOutText (lines)
//!     │                                       │
//!     │ glyph_runs()                           ▼ bounds (datatypes::Rect)
//!     ▼
//! GlyphRun (span, advances, offsets — consumed by the render graph)
//! ```
//!
//! Text is NEVER drawn in `layout-engine`: it only receives
//! extents via its `TextMeasurer` (see
//! `text_engine::measurer_for`), and the runs will feed the Render Graph
//! (text effect) without going through layout.

pub mod layout;
pub mod model;

pub use layout::{
    FixedFontProvider, FontProvider, GlyphRun, LaidOutLine, LaidOutText, ShapedGlyph, layout_text,
    measure_text,
};
pub use model::{Span, TextId, TextModel};

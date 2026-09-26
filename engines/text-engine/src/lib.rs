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

//! Moteur texte : modèle → layout → glyph runs → (render graph).
//!
//! Séparation conceptuelle (aucun façonnage réel sans fontes — les métriques
//! viennent du trait [`FontProvider`], implémenté pour de vrai plus tard) :
//!
//! ```text
//! TextModel (spans stylés) ──layout()──▶ LaidOutText (lignes)
//!     │                                       │
//!     │ glyph_runs()                           ▼ bounds (datatypes::Rect)
//!     ▼
//! GlyphRun (span, avances, offsets — consommés par le render graph)
//! ```
//!
//! Le texte n'est JAMAIS dessiné dans `layout-engine` : celui-ci ne reçoit
//! que des extensions via son `TextMeasurer` (voir
//! `text_engine::measurer_for`), et les runs alimenteront le Render Graph
//! (effet texte) sans passer par le layout.

pub mod layout;
pub mod model;

pub use layout::{
    FixedFontProvider, FontProvider, GlyphRun, LaidOutLine, LaidOutText, ShapedGlyph, layout_text,
    measure_text,
};
pub use model::{Span, TextId, TextModel};

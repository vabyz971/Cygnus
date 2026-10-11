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

//! Text model: identified styled spans, no metrics at all.
//!
//! A [`TextModel`] is a sequence of [`Span`]s (string + family + size +
//! weight/italic). No breaking, advances, or glyphs here: see
//! [`layout`](crate::layout).

use ids::EntityId;

/// Stable identifier of a [`TextModel`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct TextId(EntityId);

impl TextId {
    /// New random identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Null identifier (sentinel, never assigned).
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// The underlying entity.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Wraps an existing entity.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for TextId {
    /// Null identifier — same value as [`TextId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for TextId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "x{}", self.0.as_uuid())
    }
}

/// Styled segment: the smallest unit sharing font and weight.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    /// Text (may contain `\n` — split at layout).
    pub text: String,
    /// Font family (logical key, e.g. `"Hanken Grotesk"`).
    pub family: String,
    /// Size in points.
    pub size: f32,
    /// Weight (400 = normal, 700 = bold…).
    pub weight: u16,
    /// Italic.
    pub italic: bool,
}

impl Span {
    /// New span (negative size clamped to 0).
    #[must_use]
    pub fn new(text: impl Into<String>, family: impl Into<String>, size: f32) -> Self {
        Self {
            text: text.into(),
            family: family.into(),
            size: size.max(0.0),
            weight: 400,
            italic: false,
        }
    }
}

/// Text document: ordered sequence of spans.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextModel {
    /// Stable identifier.
    pub id: TextId,
    /// Spans in order.
    pub spans: Vec<Span>,
}

impl TextModel {
    /// New empty model.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: TextId::new(),
            spans: Vec::new(),
        }
    }

    /// True if no spans or only empty content.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|s| s.text.is_empty())
    }

    /// Adds a span.
    pub fn push(&mut self, span: Span) {
        self.spans.push(span);
    }

    /// Concatenated raw text (unstyled — diagnostics, search).
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }
}

impl Default for TextModel {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modele_concatene_et_ids() {
        let mut model = TextModel::default();
        assert!(model.is_empty());
        assert_ne!(model.id, TextId::nil());
        model.push(Span::new("Bon", "F", 12.0));
        model.push(Span::new("jour", "F", 12.0));
        assert_eq!(model.plain_text(), "Bonjour");
        assert!(!model.is_empty());
        assert_eq!(Span::new("x", "F", -4.0).size, 0.0);
    }
}

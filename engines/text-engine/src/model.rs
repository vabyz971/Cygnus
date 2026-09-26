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

//! Modèle texte : spans stylés identifiés, sans aucune métrique.
//!
//! Un [`TextModel`] est une suite de [`Span`] (chaîne + famille + corps +
//! graisse/italique). Ni coupure, ni avances, ni glyphes ici : voir
//! [`layout`](crate::layout).

use ids::EntityId;

/// Identifiant stable d'un [`TextModel`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct TextId(EntityId);

impl TextId {
    /// Nouvel identifiant aléatoire.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Identifiant nul (sentinelle, jamais attribué).
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// L'entité sous-jacente.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Enveloppe une entité existante.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for TextId {
    /// Identifiant nul — même valeur que [`TextId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for TextId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "x{}", self.0.as_uuid())
    }
}

/// Segment stylé : la plus petite unité partageant fonte et graisse.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    /// Texte (peut contenir `\n` — coupés au layout).
    pub text: String,
    /// Famille de fonte (clé logique, ex. `"Hanken Grotesk"`).
    pub family: String,
    /// Corps en points.
    pub size: f32,
    /// Graisse (400 = normal, 700 = gras…).
    pub weight: u16,
    /// Italique.
    pub italic: bool,
}

impl Span {
    /// Nouveau span (taille négative ramenée à 0).
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

/// Document texte : suite ordonnée de spans.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TextModel {
    /// Identifiant stable.
    pub id: TextId,
    /// Spans dans l'ordre.
    pub spans: Vec<Span>,
}

impl TextModel {
    /// Nouveau modèle vide.
    #[must_use]
    pub fn new() -> Self {
        Self {
            id: TextId::new(),
            spans: Vec::new(),
        }
    }

    /// Vrai si aucun span ou que du vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.spans.iter().all(|s| s.text.is_empty())
    }

    /// Ajoute un span.
    pub fn push(&mut self, span: Span) {
        self.spans.push(span);
    }

    /// Texte brut concaténé (sans style — diagnostics, recherche).
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

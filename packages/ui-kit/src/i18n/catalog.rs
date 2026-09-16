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

//! Catalogue de traductions (anglais / français).
//!
//! Tables statiques (`match`), zéro allocation : `get` retourne un
//! `&'static str`. Toute clé manquante dans une langue retombe sur
//! [`TextKey::default_text`] (anglais).

use super::key::TextKey;

/// Langue d'affichage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// Anglais (défaut, langue de repli).
    #[default]
    En,
    /// Français.
    Fr,
}

/// Catalogue lié à une langue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Catalog {
    lang: Language,
}

impl Catalog {
    /// Catalogue pour la langue `lang`.
    pub fn new(lang: Language) -> Self {
        Self { lang }
    }

    /// Langue du catalogue.
    pub fn language(self) -> Language {
        self.lang
    }

    /// Traduit `key` (repli anglais si non traduit).
    pub fn get(self, key: TextKey) -> &'static str {
        match self.lang {
            Language::En => key.default_text(),
            Language::Fr => Self::french(key),
        }
    }

    /// Table française (repli anglais pour les clés non traduites).
    fn french(key: TextKey) -> &'static str {
        match key {
            TextKey::Save => "Enregistrer",
            TextKey::Open => "Ouvrir",
            TextKey::Cancel => "Annuler",
            TextKey::Close => "Fermer",
            TextKey::Undo => "Annuler",
            TextKey::Redo => "Rétablir",
            TextKey::Delete => "Supprimer",
            TextKey::Duplicate => "Dupliquer",
            TextKey::NewDocument => "Nouveau document",
            TextKey::Export => "Exporter",
            TextKey::Layers => "Calques",
            TextKey::Settings => "Paramètres",
            TextKey::Quit => "Quitter",
            TextKey::Copy => "Copier",
            TextKey::Paste => "Coller",
            TextKey::Tools => "Outils",
            TextKey::Inspector => "Inspecteur",
            TextKey::Navigator => "Navigateur",
            TextKey::History => "Historique",
            TextKey::Timeline => "Chronologie",
        }
    }
}

impl Default for Catalog {
    /// Catalogue anglais par défaut.
    fn default() -> Self {
        Self::new(Language::En)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_matches_defaults() {
        let catalog = Catalog::new(Language::En);
        assert_eq!(catalog.get(TextKey::Save), TextKey::Save.default_text());
        assert_eq!(catalog.get(TextKey::Layers), "Layers");
    }

    #[test]
    fn french_translates_common_keys() {
        let catalog = Catalog::new(Language::Fr);
        assert_eq!(catalog.get(TextKey::Save), "Enregistrer");
        assert_eq!(catalog.get(TextKey::Layers), "Calques");
        assert_eq!(catalog.get(TextKey::Quit), "Quitter");
    }
}

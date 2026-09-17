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
//! `&'static str`. Le dispatch se fait vers [`super::en`] ou
//! [`super::fr`] ; ajouter une langue = un fichier + une variante
//! de [`Language`].

use super::{en, fr, key::TextKey};

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

    /// Traduit `key` dans la langue du catalogue.
    pub fn get(self, key: TextKey) -> &'static str {
        match self.lang {
            Language::En => en::translate(key),
            Language::Fr => fr::translate(key),
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
    fn english_matches_expected() {
        let catalog = Catalog::new(Language::En);
        assert_eq!(catalog.get(TextKey::Save), "Save");
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

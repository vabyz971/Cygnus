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

//! Translation catalog (English / French).
//!
//! Static tables (`match`), zero allocation: `get` returns a
//! `&'static str`. Dispatch goes to [`super::en`] or [`super::fr`];
//! adding a language = one file + one [`Language`] variant.

use super::{en, fr, key::TextKey};

/// Display language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Language {
    /// English (default, fallback language).
    #[default]
    En,
    /// French.
    Fr,
}

/// Catalog bound to a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Catalog {
    lang: Language,
}

impl Catalog {
    /// Catalog for language `lang`.
    pub fn new(lang: Language) -> Self {
        Self { lang }
    }

    /// Catalog's language.
    pub fn language(self) -> Language {
        self.lang
    }

    /// Translates `key` into the catalog's language.
    pub fn get(self, key: TextKey) -> &'static str {
        match self.lang {
            Language::En => en::translate(key),
            Language::Fr => fr::translate(key),
        }
    }
}

impl Default for Catalog {
    /// Default English catalog.
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
        assert_eq!(catalog.get(TextKey::Rename), "Renommer");
        assert_eq!(catalog.get(TextKey::Show), "Afficher");
        assert_eq!(catalog.get(TextKey::Hide), "Masquer");
        assert_eq!(catalog.get(TextKey::Add), "Ajouter");
        assert_eq!(catalog.get(TextKey::Create), "Créer");
        assert_eq!(catalog.get(TextKey::Validate), "Valider");
    }
}

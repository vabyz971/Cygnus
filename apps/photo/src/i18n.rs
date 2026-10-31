// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Translation catalogs of Cygnus Photo. French is the only language of the
//! beta.

use cygnus_ui_kit::i18n::{self, Catalog};

/// French catalog, the first language of the application.
const FRENCH: &str = include_str!("../i18n/fr.txt");

/// Installs the French catalog. A broken catalog is reported on stderr and
/// the interface then shows the key identifiers instead of crashing.
pub fn install() {
    let catalog = Catalog::parse(FRENCH).unwrap_or_else(|error| {
        eprintln!("cygnus-photo: invalid French catalog: {error}");
        Catalog::default()
    });
    // Only fails if a catalog is already installed, which is then kept.
    let _ = i18n::install(catalog);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keys;

    #[test]
    fn french_catalog_parses() {
        Catalog::parse(FRENCH).unwrap();
    }

    #[test]
    fn every_key_has_a_french_translation() {
        let catalog = Catalog::parse(FRENCH).unwrap();
        let missing: Vec<&str> = catalog
            .missing(keys::ALL)
            .iter()
            .map(|key| key.id())
            .collect();
        assert!(missing.is_empty(), "keys without translation: {missing:?}");
    }

    #[test]
    fn every_french_entry_is_used() {
        let catalog = Catalog::parse(FRENCH).unwrap();
        let unused = catalog.unused(keys::ALL);
        assert!(unused.is_empty(), "translations without key: {unused:?}");
    }
}

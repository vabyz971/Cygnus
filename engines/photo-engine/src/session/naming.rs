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

//! Default names of created items (layers, shapes, masks, groups,
//! live filters), supplied by the UI in its language.
//!
//! The engine stays language-free: the app sends a [`DefaultNames`]
//! (`SetDefaultNames`) when a document opens and whenever the interface
//! language changes; names only apply to items created afterwards (an
//! existing layer keeps its name, like in any editor). Without it, the
//! neutral English defaults below are used.

use std::collections::HashMap;

/// Base names used when the engine creates an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultNames {
    /// New pixel layer ("Layer 3").
    pub layer: String,
    /// New vector shape ("Shape 2").
    pub shape: String,
    /// New mask ("Mask 1").
    pub mask: String,
    /// New group.
    pub group: String,
    /// Live-filter display names by registry `type_id` (missing entries
    /// fall back to the registry name).
    pub filters: HashMap<String, String>,
}

impl Default for DefaultNames {
    fn default() -> Self {
        Self {
            layer: String::from("Layer"),
            shape: String::from("Shape"),
            mask: String::from("Mask"),
            group: String::from("Group"),
            filters: HashMap::new(),
        }
    }
}

impl DefaultNames {
    /// "`base` `n`" (numbered item names).
    #[must_use]
    pub fn numbered(base: &str, n: usize) -> String {
        format!("{base} {n}")
    }

    /// Display name of a live filter, `fallback` when not supplied.
    #[must_use]
    pub fn filter<'a>(&'a self, type_id: &str, fallback: &'a str) -> &'a str {
        self.filters.get(type_id).map_or(fallback, String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbered_and_filter_fallback() {
        let mut names = DefaultNames::default();
        assert_eq!(DefaultNames::numbered(&names.layer, 3), "Layer 3");
        assert_eq!(names.filter("levels", "Levels"), "Levels");
        names
            .filters
            .insert(String::from("levels"), String::from("Niveaux"));
        assert_eq!(names.filter("levels", "Levels"), "Niveaux");
    }
}

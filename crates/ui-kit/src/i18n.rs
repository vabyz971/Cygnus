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

//! Minimal translation system.
//!
//! Every user-visible string is looked up through a [`Key`]. Keys are declared
//! once per application with [`define_keys!`], which also produces the list of
//! all keys so that a test can check that each one has a translation.
//!
//! Catalogs are plain text files, one `key = value` entry per line. Empty
//! lines and lines starting with `#` are ignored. A value may contain named
//! placeholders such as `{count}`, filled by [`Catalog::format`].

use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

/// Identifier of a translatable string. Declare keys with [`define_keys!`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Key(&'static str);

impl Key {
    /// Creates a key. Use [`define_keys!`] instead, so that the key is listed
    /// in the application's `ALL` array and covered by the translation tests.
    #[doc(hidden)]
    pub const fn declare(id: &'static str) -> Self {
        Self(id)
    }

    /// The identifier of the key, as written in catalog files.
    pub const fn id(self) -> &'static str {
        self.0
    }
}

/// Declares translation keys as constants, plus an `ALL` array listing them.
///
/// ```
/// cygnus_ui_kit::define_keys! {
///     MENU_FILE = "menu.file";
///     MENU_EDIT = "menu.edit";
/// }
/// assert_eq!(ALL.len(), 2);
/// assert_eq!(MENU_FILE.id(), "menu.file");
/// ```
#[macro_export]
macro_rules! define_keys {
    ($($(#[$meta:meta])* $name:ident = $id:literal;)*) => {
        $(
            $(#[$meta])*
            pub const $name: $crate::i18n::Key = $crate::i18n::Key::declare($id);
        )*

        /// Every key declared in this module.
        pub const ALL: &[$crate::i18n::Key] = &[$($name),*];
    };
}

/// Error found while parsing a catalog file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// One-based line number.
    pub line: usize,
    /// What is wrong with the line.
    pub kind: ParseErrorKind,
}

/// The kind of a [`ParseError`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// The line has no `=` separator.
    MissingSeparator,
    /// The key before `=` is empty.
    EmptyKey,
    /// The key was already defined on an earlier line.
    DuplicateKey(String),
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParseErrorKind::MissingSeparator => {
                write!(f, "line {}: expected `key = value`", self.line)
            }
            ParseErrorKind::EmptyKey => write!(f, "line {}: empty key", self.line),
            ParseErrorKind::DuplicateKey(key) => {
                write!(f, "line {}: duplicate key `{key}`", self.line)
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Translations of one language.
#[derive(Debug, Clone, Default)]
pub struct Catalog {
    entries: HashMap<String, String>,
}

impl Catalog {
    /// Parses a catalog file.
    pub fn parse(source: &str) -> Result<Self, ParseError> {
        let mut entries = HashMap::new();
        for (index, raw) in source.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let error = |kind| ParseError {
                line: index + 1,
                kind,
            };
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| error(ParseErrorKind::MissingSeparator))?;
            let key = key.trim();
            if key.is_empty() {
                return Err(error(ParseErrorKind::EmptyKey));
            }
            if entries
                .insert(key.to_owned(), value.trim().to_owned())
                .is_some()
            {
                return Err(error(ParseErrorKind::DuplicateKey(key.to_owned())));
            }
        }
        Ok(Self { entries })
    }

    /// Returns the translation of `key`, or the key identifier itself when
    /// the catalog has no entry for it (visible, but never a crash).
    pub fn get(&self, key: Key) -> &str {
        self.entries.get(key.id()).map_or(key.id(), String::as_str)
    }

    /// Returns the translation of `key` with `{name}` placeholders replaced.
    pub fn format(&self, key: Key, args: &[(&str, &str)]) -> String {
        let mut text = self.get(key).to_owned();
        for (name, value) in args {
            text = text.replace(&format!("{{{name}}}"), value);
        }
        text
    }

    /// Keys of `keys` that have no translation in this catalog.
    pub fn missing(&self, keys: &[Key]) -> Vec<Key> {
        keys.iter()
            .copied()
            .filter(|key| !self.entries.contains_key(key.id()))
            .collect()
    }

    /// Catalog entries whose key is not in `keys` (dead translations).
    pub fn unused(&self, keys: &[Key]) -> Vec<String> {
        let mut unused: Vec<String> = self
            .entries
            .keys()
            .filter(|entry| !keys.iter().any(|key| key.id() == entry.as_str()))
            .cloned()
            .collect();
        unused.sort();
        unused
    }
}

static ACTIVE: OnceLock<Catalog> = OnceLock::new();

/// Installs the catalog used by [`tr`] and [`tr_format`] for the rest of the
/// process. Returns the catalog back if one was already installed.
pub fn install(catalog: Catalog) -> Result<(), Catalog> {
    ACTIVE.set(catalog)
}

/// Translates `key` with the installed catalog. Before installation, or for
/// a missing entry, returns the key identifier.
pub fn tr(key: Key) -> &'static str {
    ACTIVE.get().map_or(key.id(), |catalog| catalog.get(key))
}

/// Translates `key` with the installed catalog and fills its placeholders.
pub fn tr_format(key: Key, args: &[(&str, &str)]) -> String {
    match ACTIVE.get() {
        Some(catalog) => catalog.format(key, args),
        None => key.id().to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod keys {
        crate::define_keys! {
            HELLO = "test.hello";
            COUNT = "test.count";
            ABSENT = "test.absent";
        }
    }

    const SOURCE: &str = "\
# A comment, then an empty line.

test.hello = Bonjour
test.count = {count} calques
test.dead = Jamais utilisé
";

    #[test]
    fn parses_entries_and_ignores_comments() {
        let catalog = Catalog::parse(SOURCE).unwrap();
        assert_eq!(catalog.get(keys::HELLO), "Bonjour");
    }

    #[test]
    fn missing_entry_falls_back_to_the_key() {
        let catalog = Catalog::parse(SOURCE).unwrap();
        assert_eq!(catalog.get(keys::ABSENT), "test.absent");
    }

    #[test]
    fn formats_placeholders() {
        let catalog = Catalog::parse(SOURCE).unwrap();
        assert_eq!(catalog.format(keys::COUNT, &[("count", "4")]), "4 calques");
    }

    #[test]
    fn reports_missing_and_unused_keys() {
        let catalog = Catalog::parse(SOURCE).unwrap();
        assert_eq!(catalog.missing(keys::ALL), vec![keys::ABSENT]);
        assert_eq!(catalog.unused(keys::ALL), vec!["test.dead".to_owned()]);
    }

    #[test]
    fn rejects_malformed_lines() {
        let error = Catalog::parse("ok = yes\nno separator").unwrap_err();
        assert_eq!(error.line, 2);
        assert_eq!(error.kind, ParseErrorKind::MissingSeparator);
        let error = Catalog::parse(" = value").unwrap_err();
        assert_eq!(error.kind, ParseErrorKind::EmptyKey);
    }

    #[test]
    fn rejects_duplicate_keys() {
        let error = Catalog::parse("a = 1\na = 2").unwrap_err();
        assert_eq!(error.kind, ParseErrorKind::DuplicateKey("a".to_owned()));
    }
}

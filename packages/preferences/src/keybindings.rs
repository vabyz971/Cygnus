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

//! Generic keyboard-shortcut resolver: parsing
//! combos ("Ctrl+Shift+S") and converting a keyboard
//! event into an action id (`String`).
//!
//! The package knows NO domain action: apps define
//! their own enum (e.g. `PhotoShortcut` on the photo side, with `id()` /
//! `from_id()`) and convert the id returned by
//! [`KeybindingResolver::resolve`]. Compatibility of the preferences
//! JSON rests on these ids (`"undo"`, …).
//!
//! Framework-free keyboard types (no UI dependency): the app converts
//! `egui::Key`/`egui::Modifiers` to [`AppKey`]/[`AppModifiers`].

use std::collections::HashMap;

/// Normalized key combo ("Ctrl+Shift+S").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    /// Main key NORMALIZED TO UPPERCASE ("S", "F7", "Delete")
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl std::fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut parts = Vec::with_capacity(4);
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        parts.push(self.key.as_str());
        write!(f, "{}", parts.join("+"))
    }
}

/// UI-framework-independent logical key.
///
/// The app converts keyboard events (e.g. `egui::Key`) to this
/// type before calling [`KeybindingResolver::resolve`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppKey {
    /// Printable character ("z", "S", "+"…).
    Character(String),
    /// Named key (function, editing, arrows, modifiers…).
    Named(NamedKey),
}

/// Named keys supported by the resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    /// Function keys.
    F1,
    /// Function keys.
    F2,
    /// Function keys.
    F3,
    /// Function keys.
    F4,
    /// Function keys.
    F5,
    /// Function keys.
    F6,
    /// Function keys.
    F7,
    /// Function keys.
    F8,
    /// Function keys.
    F9,
    /// Function keys.
    F10,
    /// Function keys.
    F11,
    /// Function keys.
    F12,
    /// Space.
    Space,
    /// Enter.
    Enter,
    /// Escape.
    Escape,
    /// Delete.
    Delete,
    /// Backspace.
    Backspace,
    /// Tab.
    Tab,
    /// Up arrow.
    ArrowUp,
    /// Down arrow.
    ArrowDown,
    /// Left arrow.
    ArrowLeft,
    /// Right arrow.
    ArrowRight,
    /// Lone modifiers (never resolved to an action).
    Control,
    /// Lone modifier.
    Shift,
    /// Lone modifier.
    Alt,
    /// Lone modifier (Cmd/Super).
    Meta,
}

/// UI-framework-independent modifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AppModifiers {
    /// Ctrl (or Cmd, depending on the platform — the app chooses).
    pub ctrl: bool,
    /// Shift.
    pub shift: bool,
    /// Alt/Option.
    pub alt: bool,
    /// Explicit Cmd/Super (folded onto ctrl by the resolver).
    pub command: bool,
}

impl AppModifiers {
    /// No modifier.
    pub const EMPTY: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
        command: false,
    };

    /// Ctrl alone.
    pub const CTRL: Self = Self {
        ctrl: true,
        shift: false,
        alt: false,
        command: false,
    };
}

/// Generic resolver: combo → action-id table,
/// built from the preferences. The app converts the id
/// into its own action type (e.g. `PhotoShortcut::from_id`).
#[derive(Debug, Default)]
pub struct KeybindingResolver {
    lookup: HashMap<KeyCombo, String>,
}

impl KeybindingResolver {
    /// Builds the resolver from the id→combo table. Every
    /// parsable entry is kept (even an unknown id: this package
    /// knows nothing about apps' actions).
    #[must_use]
    pub fn from_bindings(bindings: &HashMap<String, String>) -> Self {
        let mut resolver = Self::default();
        for (action_id, combo_str) in bindings {
            if let Some(combo) = parse_combo(combo_str) {
                resolver.lookup.insert(combo, action_id.clone());
            }
        }
        resolver
    }

    /// Action id matching this keyboard event, if
    /// there is one.
    #[must_use]
    pub fn resolve(&self, key: &AppKey, modifiers: AppModifiers) -> Option<String> {
        let combo = KeyCombo {
            key: key_to_string(key)?,
            ctrl: modifiers.ctrl || modifiers.command,
            shift: modifiers.shift,
            alt: modifiers.alt,
        };
        self.lookup.get(&combo).cloned()
    }

    /// Number of active combos (diagnostics).
    #[must_use]
    pub fn len(&self) -> usize {
        self.lookup.len()
    }

    /// Always true: an empty resolver is valid.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lookup.is_empty()
    }
}

/// Parses "Ctrl+Shift+S" / "F7" / "Delete" into a normalized [`KeyCombo`].
/// Tolerates case and aliases ("Control", "Cmd", "Option").
#[must_use]
pub fn parse_combo(s: &str) -> Option<KeyCombo> {
    let mut ctrl = false;
    let mut shift = false;
    let mut alt = false;
    let mut key = String::new();

    for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" | "cmd" | "meta" => ctrl = true,
            "shift" => shift = true,
            "alt" | "option" => alt = true,
            other => key = other.to_uppercase(),
        }
    }

    if key.is_empty() {
        return None;
    }
    Some(KeyCombo {
        key,
        ctrl,
        shift,
        alt,
    })
}

/// Converts a logical key to its normalized text representation
/// (identical to the one used by [`parse_combo`]: UPPERCASE).
/// Lone modifiers return `None` (never an action).
#[must_use]
pub fn key_to_string(key: &AppKey) -> Option<String> {
    match key {
        AppKey::Character(c) => Some(c.to_uppercase()),
        AppKey::Named(named) => named_to_string(*named).map(|s| s.to_uppercase()),
    }
}

fn named_to_string(named: NamedKey) -> Option<String> {
    let s = match named {
        NamedKey::F1 => "F1",
        NamedKey::F2 => "F2",
        NamedKey::F3 => "F3",
        NamedKey::F4 => "F4",
        NamedKey::F5 => "F5",
        NamedKey::F6 => "F6",
        NamedKey::F7 => "F7",
        NamedKey::F8 => "F8",
        NamedKey::F9 => "F9",
        NamedKey::F10 => "F10",
        NamedKey::F11 => "F11",
        NamedKey::F12 => "F12",
        NamedKey::Space => "Space",
        NamedKey::Enter => "Enter",
        NamedKey::Escape => "Escape",
        NamedKey::Delete => "Delete",
        NamedKey::Backspace => "Backspace",
        NamedKey::Tab => "Tab",
        NamedKey::ArrowUp => "Up",
        NamedKey::ArrowDown => "Down",
        NamedKey::ArrowLeft => "Left",
        NamedKey::ArrowRight => "Right",
        NamedKey::Control | NamedKey::Shift | NamedKey::Alt | NamedKey::Meta => return None,
    };
    Some(s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parsing_tolerant_a_la_casse_et_aux_alias() {
        let c = parse_combo("ctrl+shift+s").expect("parse");
        assert!(c.ctrl && c.shift && !c.alt);
        assert_eq!(c.key, "S");

        let c = parse_combo("Control + Minus").expect("parse");
        assert!(c.ctrl);
        assert_eq!(c.key, "MINUS");

        assert!(parse_combo("Ctrl+").is_none(), "touche vide rejetée");
    }

    #[test]
    fn display_reconstruit_la_combinaison() {
        let c = KeyCombo {
            key: "S".into(),
            ctrl: true,
            shift: true,
            alt: false,
        };
        assert_eq!(c.to_string(), "Ctrl+Shift+S");
    }

    #[test]
    fn resolve_retourne_l_identifiant_sans_connaitre_l_action() {
        // The resolver is generic: it maps to opaque ids,
        // including ones unknown to this package.
        let mut bindings = HashMap::new();
        bindings.insert("undo".to_string(), "Ctrl+Z".to_string());
        bindings.insert("action_future_inconnue".to_string(), "F7".to_string());
        bindings.insert("invalide".to_string(), "Ctrl+".to_string());
        let resolver = KeybindingResolver::from_bindings(&bindings);
        assert_eq!(resolver.len(), 2);

        let z = AppKey::Character("z".into());
        assert_eq!(
            resolver.resolve(&z, AppModifiers::CTRL),
            Some("undo".to_string())
        );
        // 'z' WITHOUT Ctrl does not resolve.
        assert_eq!(resolver.resolve(&z, AppModifiers::EMPTY), None);
        // Unknown id kept as-is.
        assert_eq!(
            resolver.resolve(&AppKey::Named(NamedKey::F7), AppModifiers::EMPTY),
            Some("action_future_inconnue".to_string())
        );
        // Lone modifier triggers nothing.
        assert_eq!(
            resolver.resolve(&AppKey::Named(NamedKey::Control), AppModifiers::CTRL),
            None
        );
    }

    /// Named keys in mixed case (`Tab`, `Delete`, `Space`)
    /// resolve like their bindings (`TAB`, `DELETE`, `SPACE`):
    /// `parse_combo` and `key_to_string` share the same norm.
    #[test]
    fn named_keys_resolvent_malgre_la_casse_mixte() {
        let mut bindings = HashMap::new();
        bindings.insert("panneau".to_string(), "Tab".to_string());
        bindings.insert("supprimer".to_string(), "Delete".to_string());
        bindings.insert("espace".to_string(), "Space".to_string());
        let resolver = KeybindingResolver::from_bindings(&bindings);
        assert_eq!(
            resolver.resolve(&AppKey::Named(NamedKey::Tab), AppModifiers::EMPTY),
            Some("panneau".to_string())
        );
        assert_eq!(
            resolver.resolve(&AppKey::Named(NamedKey::Delete), AppModifiers::EMPTY),
            Some("supprimer".to_string())
        );
        assert_eq!(
            resolver.resolve(&AppKey::Named(NamedKey::Space), AppModifiers::EMPTY),
            Some("espace".to_string())
        );
    }
}

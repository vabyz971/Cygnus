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

//! Résolveur générique de raccourcis clavier : parsing des
//! combinaisons (« Ctrl+Shift+S ») et conversion d'un événement
//! clavier en identifiant d'action (`String`).
//!
//! Le package ne connaît AUCUNE action métier : les apps définissent
//! leur propre enum (ex. `PhotoShortcut` côté photo, avec `id()` /
//! `from_id()`) et convertissent l'identifiant retourné par
//! [`KeybindingResolver::resolve`]. La compatibilité du JSON de
//! préférences repose sur ces ids (`"undo"`, …).
//!
//! Types clavier PROPRES (aucune dépendance UI) : l'app convertit
//! `egui::Key`/`egui::Modifiers` vers [`AppKey`]/[`AppModifiers`].

use std::collections::HashMap;

/// Combinaison de touches normalisée (« Ctrl+Shift+S »).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct KeyCombo {
    /// Touche principale NORMALISÉE EN MAJUSCULE (« S », « F7 », « Delete »)
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

/// Touche logique indépendante du framework UI.
///
/// L'app convertit les événements clavier (ex. `egui::Key`) vers ce
/// type avant d'appeler [`KeybindingResolver::resolve`].
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum AppKey {
    /// Caractère imprimable (« z », « S », « + »…).
    Character(String),
    /// Touche nommée (fonction, édition, flèches, modificateurs…).
    Named(NamedKey),
}

/// Touches nommées prises en charge par le résolveur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NamedKey {
    /// Touches de fonction.
    F1,
    /// Touches de fonction.
    F2,
    /// Touches de fonction.
    F3,
    /// Touches de fonction.
    F4,
    /// Touches de fonction.
    F5,
    /// Touches de fonction.
    F6,
    /// Touches de fonction.
    F7,
    /// Touches de fonction.
    F8,
    /// Touches de fonction.
    F9,
    /// Touches de fonction.
    F10,
    /// Touches de fonction.
    F11,
    /// Touches de fonction.
    F12,
    /// Espace.
    Space,
    /// Entrée.
    Enter,
    /// Échap.
    Escape,
    /// Suppr.
    Delete,
    /// Retour arrière.
    Backspace,
    /// Tabulation.
    Tab,
    /// Flèche haut.
    ArrowUp,
    /// Flèche bas.
    ArrowDown,
    /// Flèche gauche.
    ArrowLeft,
    /// Flèche droite.
    ArrowRight,
    /// Modificateurs seuls (jamais résolus en action).
    Control,
    /// Modificateur seul.
    Shift,
    /// Modificateur seul.
    Alt,
    /// Modificateur seul (Cmd/Super).
    Meta,
}

/// Modificateurs indépendants du framework UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct AppModifiers {
    /// Ctrl (ou Cmd, selon la plateforme — l'app choisit).
    pub ctrl: bool,
    /// Maj.
    pub shift: bool,
    /// Alt/Option.
    pub alt: bool,
    /// Cmd/Super explicite (replié sur ctrl par le résolveur).
    pub command: bool,
}

impl AppModifiers {
    /// Aucun modificateur.
    pub const EMPTY: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
        command: false,
    };

    /// Ctrl seul.
    pub const CTRL: Self = Self {
        ctrl: true,
        shift: false,
        alt: false,
        command: false,
    };
}

/// Résolveur générique : table combo → identifiant d'action,
/// construite depuis les préférences. L'app convertit l'identifiant
/// en son propre type d'action (ex. `PhotoShortcut::from_id`).
#[derive(Debug, Default)]
pub struct KeybindingResolver {
    lookup: HashMap<KeyCombo, String>,
}

impl KeybindingResolver {
    /// Construit le résolveur depuis la table id→combinaison. Toute
    /// entrée parsable est conservée (même un id inconnu : ce package
    /// ne connaît pas les actions des apps).
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

    /// Identifiant d'action correspondant à cet événement clavier, s'il
    /// y en a un.
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

    /// Nombre de combinaisons actives (diagnostic).
    #[must_use]
    pub fn len(&self) -> usize {
        self.lookup.len()
    }

    /// Toujours vrai : un résolveur vide est valide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lookup.is_empty()
    }
}

/// Parse « Ctrl+Shift+S » / « F7 » / « Delete » en [`KeyCombo`] normalisé.
/// Tolère la casse et les alias (« Control », « Cmd », « Option »).
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

/// Convertit une touche logique en sa représentation texte normalisée
/// (identique à celle utilisée par [`parse_combo`] : MAJUSCULES).
/// Les modificateurs seuls retournent `None` (jamais d'action).
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
        // Le résolveur est générique : il mappe vers des ids opaques,
        // y compris inconnus de ce package.
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
        // 'z' SANS Ctrl ne résout pas.
        assert_eq!(resolver.resolve(&z, AppModifiers::EMPTY), None);
        // Id inconnu conservé tel quel.
        assert_eq!(
            resolver.resolve(&AppKey::Named(NamedKey::F7), AppModifiers::EMPTY),
            Some("action_future_inconnue".to_string())
        );
        // Modificateur seul ne déclenche rien.
        assert_eq!(
            resolver.resolve(&AppKey::Named(NamedKey::Control), AppModifiers::CTRL),
            None
        );
    }

    /// Les touches nommées en casse mixte (`Tab`, `Delete`, `Space`)
    /// résolvent comme leurs bindings (`TAB`, `DELETE`, `SPACE`) :
    /// `parse_combo` et `key_to_string` partagent la même norme.
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

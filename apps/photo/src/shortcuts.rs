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

//! Shortcuttable actions of the photo app.
//!
//! [`PhotoShortcut`] is the typed counterpart of the serialized
//! identifiers (`"undo"`, …) stored in preferences: the generic
//! [`KeybindingResolver`](preferences::KeybindingResolver)
//! returns an id, [`shortcut_for`] converts it into an action. The ids
//! are the preferences-JSON compatibility key — never
//! rename them without a migration.
//!
//! Wiring (O002): [`poll_shortcut_actions`] converts the
//! frame's egui keyboard events into [`PhotoAction`] via the
//! resolver; `PhotoApp::draw` pushes the result into the queue
//! (same flow as menus and panels). `Escape` stays handled locally
//! by widgets (stroke/rename cancellation).

#![allow(dead_code)]

use crate::commands::PhotoAction;
use crate::layout::dock::PhotoDockTab;
use crate::ui::PhotoCanvasTool;
use preferences::{AppKey, AppModifiers, KeybindingResolver, NamedKey};

/// Every shortcuttable action of the photo app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhotoShortcut {
    // Tools
    ToolBrush,
    ToolEraser,
    ToolEyedropper,
    ToolMove,
    ToolNode,
    ToolHand,
    ToolZoom,
    // Edit
    Undo,
    Redo,
    DeleteLayer,
    // File
    NewProject,
    Open,
    Save,
    SaveAs,
    Export,
    // View
    ZoomIn,
    ZoomOut,
    ZoomFit,
    Zoom100,
    ToggleLayersPanel,
    ToggleToolsPanel,
    // Layers
    NewLayer,
    DuplicateLayer,
    // Application
    OpenPreferences,
}

impl PhotoShortcut {
    /// Every action, stable display order in the window.
    pub const ALL: [PhotoShortcut; 24] = [
        PhotoShortcut::ToolBrush,
        PhotoShortcut::ToolEraser,
        PhotoShortcut::ToolEyedropper,
        PhotoShortcut::ToolMove,
        PhotoShortcut::ToolNode,
        PhotoShortcut::ToolHand,
        PhotoShortcut::ToolZoom,
        PhotoShortcut::Undo,
        PhotoShortcut::Redo,
        PhotoShortcut::DeleteLayer,
        PhotoShortcut::NewProject,
        PhotoShortcut::Open,
        PhotoShortcut::Save,
        PhotoShortcut::SaveAs,
        PhotoShortcut::Export,
        PhotoShortcut::ZoomIn,
        PhotoShortcut::ZoomOut,
        PhotoShortcut::ZoomFit,
        PhotoShortcut::Zoom100,
        PhotoShortcut::ToggleLayersPanel,
        PhotoShortcut::ToggleToolsPanel,
        PhotoShortcut::NewLayer,
        PhotoShortcut::DuplicateLayer,
        PhotoShortcut::OpenPreferences,
    ];

    /// Serialized id (bindings-table key, stable).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::ToolBrush => "tool_brush",
            Self::ToolEraser => "tool_eraser",
            Self::ToolEyedropper => "tool_eyedropper",
            Self::ToolMove => "tool_move",
            Self::ToolNode => "tool_node",
            Self::ToolHand => "tool_hand",
            Self::ToolZoom => "tool_zoom",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::DeleteLayer => "delete_layer",
            Self::NewProject => "new_project",
            Self::Open => "open",
            Self::Save => "save",
            Self::SaveAs => "save_as",
            Self::Export => "export",
            Self::ZoomIn => "zoom_in",
            Self::ZoomOut => "zoom_out",
            Self::ZoomFit => "zoom_fit",
            Self::Zoom100 => "zoom_100",
            Self::ToggleLayersPanel => "toggle_layers_panel",
            Self::ToggleToolsPanel => "toggle_tools_panel",
            Self::NewLayer => "new_layer",
            Self::DuplicateLayer => "duplicate_layer",
            Self::OpenPreferences => "open_preferences",
        }
    }

    /// Label shown to the user.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ToolBrush => "Outil Pinceau",
            Self::ToolEraser => "Outil Gomme",
            Self::ToolEyedropper => "Pipette",
            Self::ToolMove => "Déplacement",
            Self::ToolNode => "Nœuds",
            Self::ToolHand => "Main",
            Self::ToolZoom => "Zoom",
            Self::Undo => "Annuler",
            Self::Redo => "Rétablir",
            Self::DeleteLayer => "Supprimer le calque",
            Self::NewProject => "Nouveau projet",
            Self::Open => "Ouvrir",
            Self::Save => "Enregistrer",
            Self::SaveAs => "Enregistrer sous",
            Self::Export => "Exporter l'image",
            Self::ZoomIn => "Zoom avant",
            Self::ZoomOut => "Zoom arrière",
            Self::ZoomFit => "Ajuster à l'écran",
            Self::Zoom100 => "Zoom 100 %",
            Self::ToggleLayersPanel => "Panneau Calques",
            Self::ToggleToolsPanel => "Barre d'outils",
            Self::NewLayer => "Nouveau calque",
            Self::DuplicateLayer => "Dupliquer le calque",
            Self::OpenPreferences => "Préférences",
        }
    }

    /// Category for visual grouping.
    #[must_use]
    pub fn category(self) -> &'static str {
        match self {
            Self::ToolBrush
            | Self::ToolEraser
            | Self::ToolEyedropper
            | Self::ToolMove
            | Self::ToolNode
            | Self::ToolHand
            | Self::ToolZoom => "Outils",
            Self::Undo | Self::Redo | Self::DeleteLayer => "Édition",
            Self::NewProject | Self::Open | Self::Save | Self::SaveAs | Self::Export => "Fichier",
            Self::ZoomIn
            | Self::ZoomOut
            | Self::ZoomFit
            | Self::Zoom100
            | Self::ToggleLayersPanel
            | Self::ToggleToolsPanel => "Affichage",
            Self::NewLayer | Self::DuplicateLayer => "Calques",
            Self::OpenPreferences => "Application",
        }
    }

    /// Finds the action from its serialized id.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.id() == id)
    }
}

/// Converts a keyboard event into a photo action via the generic
/// resolver (id → [`PhotoShortcut`]).
#[must_use]
pub fn shortcut_for(
    resolver: &KeybindingResolver,
    key: &AppKey,
    modifiers: AppModifiers,
) -> Option<PhotoShortcut> {
    resolver
        .resolve(key, modifiers)
        .and_then(|id| PhotoShortcut::from_id(&id))
}

impl PhotoShortcut {
    /// Converts the shortcut into an app action (`None` = no current
    /// target: `ZoomFit` waits for content framing, `OpenPreferences`
    /// waits for a preferences panel — future goals, not
    /// oversights).
    #[must_use]
    pub fn to_action(self) -> Option<PhotoAction> {
        Some(match self {
            Self::ToolBrush => PhotoAction::SetTool(PhotoCanvasTool::Brush),
            Self::ToolEraser => PhotoAction::SetTool(PhotoCanvasTool::Eraser),
            Self::ToolEyedropper => PhotoAction::SetTool(PhotoCanvasTool::Eyedropper),
            Self::ToolMove => PhotoAction::SetTool(PhotoCanvasTool::Move),
            Self::ToolNode => PhotoAction::SetTool(PhotoCanvasTool::Node),
            Self::ToolHand => PhotoAction::SetTool(PhotoCanvasTool::Pan),
            Self::ToolZoom => PhotoAction::SetTool(PhotoCanvasTool::Zoom),
            Self::Undo => PhotoAction::Undo,
            Self::Redo => PhotoAction::Redo,
            Self::DeleteLayer => PhotoAction::DeleteSelectedLayer,
            Self::NewProject => PhotoAction::OpenNewDocumentDialog,
            Self::Open => PhotoAction::OpenProjectDialog,
            Self::Save => PhotoAction::SaveProject,
            Self::SaveAs => PhotoAction::SaveProjectAsDialog,
            Self::Export => PhotoAction::OpenExportDialog,
            Self::ZoomIn => PhotoAction::ZoomIn,
            Self::ZoomOut => PhotoAction::ZoomOut,
            Self::Zoom100 => PhotoAction::ZoomReset,
            Self::ToggleLayersPanel => PhotoAction::ShowDockTab(PhotoDockTab::Layers),
            Self::ToggleToolsPanel => PhotoAction::ShowDockTab(PhotoDockTab::Tools),
            Self::NewLayer => PhotoAction::AddEmptyLayer,
            Self::DuplicateLayer => PhotoAction::DuplicateSelectedLayer,
            Self::ZoomFit | Self::OpenPreferences => return None,
        })
    }
}

/// Converts an egui key into a logical key (`None` = ignored:
/// neither bound by default nor exposed by the resolver).
///
/// Spelling follows the defaults (`model.rs`): `"Plus"` (→ `PLUS`),
/// `"Minus"`, `","`, digits `"0"`…` (case-insensitive via
/// `key_to_string`).
#[must_use]
pub fn egui_key_to_app(key: egui::Key) -> Option<AppKey> {
    use egui::Key as K;
    let character = |s: &str| AppKey::Character(s.to_string());
    let named = |n: NamedKey| AppKey::Named(n);
    Some(match key {
        K::A => character("a"),
        K::B => character("b"),
        K::C => character("c"),
        K::D => character("d"),
        K::E => character("e"),
        K::F => character("f"),
        K::G => character("g"),
        K::H => character("h"),
        K::I => character("i"),
        K::J => character("j"),
        K::K => character("k"),
        K::L => character("l"),
        K::M => character("m"),
        K::N => character("n"),
        K::O => character("o"),
        K::P => character("p"),
        K::Q => character("q"),
        K::R => character("r"),
        K::S => character("s"),
        K::T => character("t"),
        K::U => character("u"),
        K::V => character("v"),
        K::W => character("w"),
        K::X => character("x"),
        K::Y => character("y"),
        K::Z => character("z"),
        K::Num0 => character("0"),
        K::Num1 => character("1"),
        K::Num2 => character("2"),
        K::Num3 => character("3"),
        K::Num4 => character("4"),
        K::Num5 => character("5"),
        K::Num6 => character("6"),
        K::Num7 => character("7"),
        K::Num8 => character("8"),
        K::Num9 => character("9"),
        K::Plus => character("Plus"),
        K::Minus => character("Minus"),
        K::Comma => character(","),
        K::F1 => named(NamedKey::F1),
        K::F2 => named(NamedKey::F2),
        K::F3 => named(NamedKey::F3),
        K::F4 => named(NamedKey::F4),
        K::F5 => named(NamedKey::F5),
        K::F6 => named(NamedKey::F6),
        K::F7 => named(NamedKey::F7),
        K::F8 => named(NamedKey::F8),
        K::F9 => named(NamedKey::F9),
        K::F10 => named(NamedKey::F10),
        K::F11 => named(NamedKey::F11),
        K::F12 => named(NamedKey::F12),
        K::Tab => named(NamedKey::Tab),
        K::Delete => named(NamedKey::Delete),
        K::Backspace => named(NamedKey::Backspace),
        K::Space => named(NamedKey::Space),
        K::Enter => named(NamedKey::Enter),
        K::Escape => named(NamedKey::Escape),
        _ => return None,
    })
}

/// Converts egui modifiers to the logical model
/// (`command` ⌘ folded onto ctrl, like the resolver).
#[must_use]
pub fn egui_modifiers_to_app(modifiers: egui::Modifiers) -> AppModifiers {
    AppModifiers {
        ctrl: modifiers.ctrl || modifiers.command,
        shift: modifiers.shift,
        alt: modifiers.alt,
        command: modifiers.command,
    }
}

/// Frame keyboard actions: pressed `Key` events (no
/// repeat) resolved then converted.
///
/// D007 guard: text entry in progress (`wants_keyboard_input`, e.g.
/// layer rename) = no shortcut — typing `b` must not
/// switch tools.
#[must_use]
pub fn poll_shortcut_actions(
    ctx: &egui::Context,
    resolver: &KeybindingResolver,
) -> Vec<PhotoAction> {
    if ctx.egui_wants_keyboard_input() {
        return Vec::new();
    }
    let mut actions = Vec::new();
    ctx.input(|input| {
        for event in &input.events {
            if let egui::Event::Key {
                key,
                pressed: true,
                repeat: false,
                modifiers,
                ..
            } = event
            {
                let (Some(app_key), mods) =
                    (egui_key_to_app(*key), egui_modifiers_to_app(*modifiers))
                else {
                    continue;
                };
                if let Some(shortcut) = shortcut_for(resolver, &app_key, mods)
                    && let Some(action) = shortcut.to_action()
                {
                    actions.push(action);
                }
            }
        }
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use preferences::NamedKey;

    #[test]
    fn meta_coherence_sur_toutes_les_actions() {
        for action in PhotoShortcut::ALL {
            assert_eq!(
                PhotoShortcut::from_id(action.id()),
                Some(action),
                "from_id(id()) doit être l'identité"
            );
            assert!(!action.label().is_empty());
            assert!(!action.category().is_empty());
        }
    }

    #[test]
    fn resolve_trouve_les_raccourcis_par_defaut() {
        let defaults = preferences::KeybindingPreferences::with_defaults();
        let resolver = KeybindingResolver::from_bindings(&defaults.bindings);

        // Ctrl+Z → Undo
        let z = AppKey::Character("z".into());
        assert_eq!(
            shortcut_for(&resolver, &z, AppModifiers::CTRL),
            Some(PhotoShortcut::Undo)
        );

        // 'b' with no modifier → ToolBrush
        let b = AppKey::Character("b".into());
        assert_eq!(
            shortcut_for(&resolver, &b, AppModifiers::EMPTY),
            Some(PhotoShortcut::ToolBrush)
        );

        // F7 → layers panel
        assert_eq!(
            shortcut_for(&resolver, &AppKey::Named(NamedKey::F7), AppModifiers::EMPTY),
            Some(PhotoShortcut::ToggleLayersPanel)
        );
    }

    #[test]
    fn resolution_sensible_aux_modificateurs() {
        let defaults = preferences::KeybindingPreferences::with_defaults();
        let resolver = KeybindingResolver::from_bindings(&defaults.bindings);
        // 's' WITHOUT Ctrl must NOT trigger Save
        let s = AppKey::Character("s".into());
        assert_ne!(
            shortcut_for(&resolver, &s, AppModifiers::EMPTY),
            Some(PhotoShortcut::Save)
        );
        assert_eq!(
            shortcut_for(&resolver, &s, AppModifiers::CTRL),
            Some(PhotoShortcut::Save)
        );
    }

    /// All 24 shortcuts map to an action, except the 2 documented
    /// gaps (`ZoomFit`, `OpenPreferences`).
    #[test]
    fn every_shortcut_maps_or_is_documented_gap() {
        let mut mapped = 0;
        for shortcut in PhotoShortcut::ALL {
            match shortcut.to_action() {
                Some(_) => mapped += 1,
                None => assert!(
                    matches!(
                        shortcut,
                        PhotoShortcut::ZoomFit | PhotoShortcut::OpenPreferences
                    ),
                    "trou non documenté : {shortcut:?}"
                ),
            }
        }
        assert_eq!(mapped, 22);
    }

    /// Targeted conversions: tools, file (O001), layers, panels.
    #[test]
    fn mapping_targets_expected_actions() {
        use crate::commands::PhotoAction;
        use crate::layout::dock::PhotoDockTab;
        use crate::ui::PhotoCanvasTool;
        assert_eq!(
            PhotoShortcut::Save.to_action(),
            Some(PhotoAction::SaveProject)
        );
        assert_eq!(
            PhotoShortcut::SaveAs.to_action(),
            Some(PhotoAction::SaveProjectAsDialog)
        );
        assert_eq!(
            PhotoShortcut::Open.to_action(),
            Some(PhotoAction::OpenProjectDialog)
        );
        assert_eq!(
            PhotoShortcut::ToolHand.to_action(),
            Some(PhotoAction::SetTool(PhotoCanvasTool::Pan))
        );
        assert_eq!(
            PhotoShortcut::ToggleLayersPanel.to_action(),
            Some(PhotoAction::ShowDockTab(PhotoDockTab::Layers))
        );
        assert_eq!(
            PhotoShortcut::DuplicateLayer.to_action(),
            Some(PhotoAction::DuplicateSelectedLayer)
        );
    }

    /// The defaults' egui keys resolve (letters, digits,
    /// Plus/Minus, comma, F7, Tab, Delete).
    #[test]
    fn egui_keys_resolve_to_bound_shortcuts() {
        use egui::Key as K;
        let defaults = preferences::KeybindingPreferences::with_defaults();
        let resolver = KeybindingResolver::from_bindings(&defaults.bindings);
        let ctrl_shift = AppModifiers {
            ctrl: true,
            shift: true,
            ..AppModifiers::EMPTY
        };
        let key = |k: K| egui_key_to_app(k).expect("touche couverte");
        assert_eq!(
            shortcut_for(&resolver, &key(K::B), AppModifiers::EMPTY),
            Some(PhotoShortcut::ToolBrush)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::S), AppModifiers::CTRL),
            Some(PhotoShortcut::Save)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::S), ctrl_shift),
            Some(PhotoShortcut::SaveAs)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::Plus), AppModifiers::CTRL),
            Some(PhotoShortcut::ZoomIn)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::Minus), AppModifiers::CTRL),
            Some(PhotoShortcut::ZoomOut)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::Num1), AppModifiers::CTRL),
            Some(PhotoShortcut::Zoom100)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::F7), AppModifiers::EMPTY),
            Some(PhotoShortcut::ToggleLayersPanel)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::Tab), AppModifiers::EMPTY),
            Some(PhotoShortcut::ToggleToolsPanel)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::Delete), AppModifiers::EMPTY),
            Some(PhotoShortcut::DeleteLayer)
        );
        assert_eq!(
            shortcut_for(&resolver, &key(K::Comma), AppModifiers::CTRL),
            Some(PhotoShortcut::OpenPreferences)
        );
        // Off-table key: ignored (never a surprise action).
        assert_eq!(egui_key_to_app(K::Copy), None);
    }

    /// Frame dispatch: Ctrl+S yields `SaveProject`, but nothing comes
    /// out during text entry (D007).
    #[test]
    fn dispatch_delivers_and_suspends_on_text_entry() {
        use crate::commands::PhotoAction;
        use egui::RawInput;
        let defaults = preferences::KeybindingPreferences::with_defaults();
        let resolver = KeybindingResolver::from_bindings(&defaults.bindings);
        let ctx = egui::Context::default();
        let save = egui::Event::Key {
            key: egui::Key::S,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                ..Default::default()
            },
        };
        // No entry: the action comes out.
        let input = RawInput {
            events: vec![save.clone()],
            ..Default::default()
        };
        let mut actions = Vec::new();
        ctx.run_ui(input, |ui| {
            actions = poll_shortcut_actions(ui.ctx(), &resolver);
        })
        .drop_without_applying_deltas();
        assert_eq!(actions, vec![PhotoAction::SaveProject]);
        // Entry in progress (rename): the same key is ignored.
        let input = RawInput {
            events: vec![save],
            ..Default::default()
        };
        let mut actions = Vec::new();
        ctx.run_ui(input, |ui| {
            let mut name = String::from("Calque 1");
            let edit = egui::TextEdit::singleline(&mut name).show(ui);
            edit.response.request_focus();
            actions = poll_shortcut_actions(ui.ctx(), &resolver);
        })
        .drop_without_applying_deltas();
        assert!(actions.is_empty(), "saisie texte = pas de raccourci");
    }
}

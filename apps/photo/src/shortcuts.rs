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

//! Actions raccourcissables de l'app photo.
//!
//! [`PhotoShortcut`] est la contrepartie typée des identifiants
//! sérialisés (`"undo"`, …) stockés dans les préférences : le
//! [`KeybindingResolver`](preferences::KeybindingResolver) générique
//! retourne un id, [`shortcut_for`] le convertit en action. Les ids
//! sont la clé de compatibilité du JSON de préférences — ne jamais
//! les renommer sans migration.
//!
//! Le câblage dans la boucle UI (dispatch clavier → `PhotoAction`)
//! viendra dans une passe ultérieure : ce module est la fondation
//! testée, pas encore branchée.

#![allow(dead_code)]

use preferences::{AppKey, AppModifiers, KeybindingResolver};

/// Toutes les actions raccourcissables de l'app photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhotoShortcut {
    // Outils
    ToolBrush,
    ToolEraser,
    ToolEyedropper,
    ToolMove,
    ToolHand,
    ToolZoom,
    // Édition
    Undo,
    Redo,
    DeleteLayer,
    // Fichier
    NewProject,
    Open,
    Save,
    SaveAs,
    Export,
    // Affichage
    ZoomIn,
    ZoomOut,
    ZoomFit,
    Zoom100,
    ToggleLayersPanel,
    ToggleToolsPanel,
    // Calques
    NewLayer,
    DuplicateLayer,
    // Application
    OpenPreferences,
}

impl PhotoShortcut {
    /// Toutes les actions, ordre d'affichage stable dans la fenêtre.
    pub const ALL: [PhotoShortcut; 23] = [
        PhotoShortcut::ToolBrush,
        PhotoShortcut::ToolEraser,
        PhotoShortcut::ToolEyedropper,
        PhotoShortcut::ToolMove,
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

    /// Identifiant sérialisé (clé de la table de bindings, stable).
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::ToolBrush => "tool_brush",
            Self::ToolEraser => "tool_eraser",
            Self::ToolEyedropper => "tool_eyedropper",
            Self::ToolMove => "tool_move",
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

    /// Libellé affiché à l'utilisateur.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::ToolBrush => "Outil Pinceau",
            Self::ToolEraser => "Outil Gomme",
            Self::ToolEyedropper => "Pipette",
            Self::ToolMove => "Déplacement",
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

    /// Catégorie pour le groupement visuel.
    #[must_use]
    pub fn category(self) -> &'static str {
        match self {
            Self::ToolBrush
            | Self::ToolEraser
            | Self::ToolEyedropper
            | Self::ToolMove
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

    /// Retrouve l'action depuis son identifiant sérialisé.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|a| a.id() == id)
    }
}

/// Convertit un événement clavier en action photo via le résolveur
/// générique (id → [`PhotoShortcut`]).
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

        // 'b' sans modificateur → ToolBrush
        let b = AppKey::Character("b".into());
        assert_eq!(
            shortcut_for(&resolver, &b, AppModifiers::EMPTY),
            Some(PhotoShortcut::ToolBrush)
        );

        // F7 → panneau calques
        assert_eq!(
            shortcut_for(&resolver, &AppKey::Named(NamedKey::F7), AppModifiers::EMPTY),
            Some(PhotoShortcut::ToggleLayersPanel)
        );
    }

    #[test]
    fn resolution_sensible_aux_modificateurs() {
        let defaults = preferences::KeybindingPreferences::with_defaults();
        let resolver = KeybindingResolver::from_bindings(&defaults.bindings);
        // 's' SANS Ctrl ne doit PAS déclencher Enregistrer
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
}

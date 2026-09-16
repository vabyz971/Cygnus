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

//! État global du workspace : régions + panneaux.
//!
//! Volontairement simple (pas de docking) : une région latérale
//! gauche, une région latérale droite, une région basse et la vue
//! centrale. Les apps dessinent leurs `egui::Panel` depuis cet état
//! et le mémorisent via [`super::persistence`] + `preferences`.

use super::panel_id::PanelId;
use super::panel_state::PanelState;
use serde::{Deserialize, Serialize};

/// Version du format de `WorkspaceState` (incrémentée à tout
/// changement incompatible, refus propre au chargement).
pub const WORKSPACE_VERSION: u32 = 1;

/// État global sérialisable du layout.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceState {
    /// Version du format.
    pub version: u32,
    /// Panneaux de la région gauche.
    #[serde(default)]
    pub left: Vec<PanelState>,
    /// Panneaux de la région droite.
    #[serde(default)]
    pub right: Vec<PanelState>,
    /// Panneaux de la région basse.
    #[serde(default)]
    pub bottom: Vec<PanelState>,
}

impl WorkspaceState {
    /// Layout par défaut : outils à gauche, inspecteur + calques à
    /// droite, timeline en bas.
    pub fn defaults() -> Self {
        Self {
            version: WORKSPACE_VERSION,
            left: vec![PanelState::new(PanelId::Tools)],
            right: vec![
                PanelState::new(PanelId::Inspector),
                PanelState::new(PanelId::Layers),
            ],
            bottom: vec![PanelState::new(PanelId::Timeline)],
        }
    }

    /// Trouve l'état mutable d'un panneau (toutes régions).
    pub fn find_mut(&mut self, id: PanelId) -> Option<&mut PanelState> {
        self.left
            .iter_mut()
            .chain(self.right.iter_mut())
            .chain(self.bottom.iter_mut())
            .find(|panel| panel.id == id)
    }

    /// Trouve l'état d'un panneau (toutes régions).
    pub fn find(&self, id: PanelId) -> Option<&PanelState> {
        self.left
            .iter()
            .chain(self.right.iter())
            .chain(self.bottom.iter())
            .find(|panel| panel.id == id)
    }

    /// Bascule la visibilité d'un panneau ; `false` si inconnu.
    pub fn toggle(&mut self, id: PanelId) -> bool {
        match self.find_mut(id) {
            Some(panel) => {
                panel.toggle();
                true
            }
            None => false,
        }
    }

    /// Change la visibilité d'un panneau ; `false` si inconnu.
    pub fn set_visible(&mut self, id: PanelId, visible: bool) -> bool {
        match self.find_mut(id) {
            Some(panel) => {
                panel.visible = visible;
                true
            }
            None => false,
        }
    }
}

impl Default for WorkspaceState {
    /// Layout par défaut.
    fn default() -> Self {
        Self::defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_contain_expected_panels() {
        let workspace = WorkspaceState::defaults();
        assert_eq!(workspace.version, WORKSPACE_VERSION);
        assert!(workspace.find(PanelId::Tools).is_some());
        assert!(workspace.find(PanelId::Layers).is_some());
        assert!(workspace.find(PanelId::Timeline).is_some());
    }

    #[test]
    fn toggle_flips_visibility() {
        let mut workspace = WorkspaceState::defaults();
        assert!(workspace.find(PanelId::Layers).is_some_and(|p| p.visible));
        assert!(workspace.toggle(PanelId::Layers));
        assert!(workspace.find(PanelId::Layers).is_some_and(|p| !p.visible));
        assert!(!workspace.toggle(PanelId::History));
    }
}

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

//! Global workspace state: regions + panels.
//!
//! Deliberately simple (no docking): a left side region, a right
//! side region, a bottom region and the central view. Apps draw
//! their `egui::Panel` from this state and remember it via
//! [`super::persistence`] + `preferences`.

use super::panel_id::PanelId;
use super::panel_state::PanelState;
use serde::{Deserialize, Serialize};

/// `WorkspaceState` format version (bumped on any incompatible
/// change, clean refusal on load).
pub const WORKSPACE_VERSION: u32 = 1;

/// Serializable global layout state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkspaceState {
    /// Format version.
    pub version: u32,
    /// Left-region panels.
    #[serde(default)]
    pub left: Vec<PanelState>,
    /// Right-region panels.
    #[serde(default)]
    pub right: Vec<PanelState>,
    /// Bottom-region panels.
    #[serde(default)]
    pub bottom: Vec<PanelState>,
}

impl WorkspaceState {
    /// Default layout: tools on the left, inspector + layers on the
    /// right, timeline at the bottom.
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

    /// Finds a panel's mutable state (all regions).
    pub fn find_mut(&mut self, id: PanelId) -> Option<&mut PanelState> {
        self.left
            .iter_mut()
            .chain(self.right.iter_mut())
            .chain(self.bottom.iter_mut())
            .find(|panel| panel.id == id)
    }

    /// Finds a panel's state (all regions).
    pub fn find(&self, id: PanelId) -> Option<&PanelState> {
        self.left
            .iter()
            .chain(self.right.iter())
            .chain(self.bottom.iter())
            .find(|panel| panel.id == id)
    }

    /// Toggles a panel's visibility; `false` when unknown.
    pub fn toggle(&mut self, id: PanelId) -> bool {
        match self.find_mut(id) {
            Some(panel) => {
                panel.toggle();
                true
            }
            None => false,
        }
    }

    /// Sets a panel's visibility; `false` when unknown.
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
    /// Default layout.
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

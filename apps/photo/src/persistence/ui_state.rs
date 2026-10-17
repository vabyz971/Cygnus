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

//! Workspace JSON save / restore.
//!
//! Errors never surface at boot: a corrupt or
//! differently-versioned JSON falls back to the default layout
//! ([`load_workspace_or_default`], [`load_dock_or_default`]).

use crate::layout::dock::{
    PhotoDockTab, default_tree, ensure_histogram_tabbed, ensure_history_tabbed, reconcile_canvases,
};
use cygnus_ui::layout::{WorkspaceState, load_workspace, save_workspace};
use egui_tiles::Tree;
use uuid::Uuid;

/// Serializes the workspace to JSON.
///
/// Waiting on disk wiring (`preferences`): next phase.
///
/// # Errors
/// Returns the `serde_json` error when serialization fails.
#[allow(dead_code)]
pub fn save_workspace_state(state: &WorkspaceState) -> Result<String, serde_json::Error> {
    save_workspace(state)
}

/// Restores the workspace (error on invalid JSON or incompatible
/// version — see [`load_workspace_or_default`]).
///
/// Waiting on disk wiring (`preferences`): next phase.
///
/// # Errors
/// Returns an error when the JSON is invalid or rejected by cygnus-ui.
#[allow(dead_code)]
pub fn load_workspace_state(json: &str) -> Result<WorkspaceState, serde_json::Error> {
    load_workspace(json)
}

/// Restores the workspace, or the default layout when the JSON is
/// missing or invalid (boot must never fail over a
/// layout).
pub fn load_workspace_or_default(json: Option<&str>) -> WorkspaceState {
    json.and_then(|raw| load_workspace(raw).ok())
        .unwrap_or_default()
}

/// Serializes the tile arrangement to JSON.
///
/// `egui_tiles` persists only structure (shares, visibilities),
/// no infinite geometry: the save is valid even before
/// the first layout.
///
/// Waiting on disk wiring (`preferences`): next phase.
///
/// # Errors
/// Returns the `serde_json` error when serialization fails.
#[allow(dead_code)]
pub fn save_tree_state(state: &Tree<PhotoDockTab>) -> Result<String, serde_json::Error> {
    serde_json::to_string(state)
}

/// Restores the tile arrangement (error on invalid JSON —
/// see [`load_tree_or_default`]).
///
/// Waiting on disk wiring (`preferences`): next phase.
///
/// # Errors
/// Returns an error when the JSON is invalid.
#[allow(dead_code)]
pub fn load_tree_state(json: &str) -> Result<Tree<PhotoDockTab>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Restores the tile arrangement, or the default layout when the
/// JSON is missing or invalid. Document ids change on
/// every launch: restored canvases are reconciled with
/// `ids` (orphans removed, missing ones added). JSON predating
/// the History tab gains it as a Layers tab.
pub fn load_tree_or_default(json: Option<&str>, ids: &[Uuid]) -> Tree<PhotoDockTab> {
    let mut state = json
        .and_then(|raw| load_tree_state(raw).ok())
        .unwrap_or_else(|| default_tree(ids.iter().map(|id| PhotoDockTab::Canvas(*id)).collect()));
    reconcile_canvases(&mut state, ids);
    ensure_history_tabbed(&mut state);
    ensure_histogram_tabbed(&mut state);
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_preserves_workspace() {
        let state = WorkspaceState::defaults();
        let json = save_workspace_state(&state).expect("serialisation");
        assert_eq!(load_workspace_state(&json).expect("restauration"), state);
    }

    #[test]
    fn corrupt_or_missing_json_falls_back_to_defaults() {
        assert_eq!(
            load_workspace_or_default(Some("pas du json")),
            WorkspaceState::defaults()
        );
        assert_eq!(load_workspace_or_default(None), WorkspaceState::defaults());
    }
}

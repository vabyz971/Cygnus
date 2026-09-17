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

//! Sauvegarde / restauration JSON du workspace.
//!
//! Les erreurs ne remontent jamais au boot : un JSON corrompu ou
//! versionné différemment retombe sur le layout par défaut
//! ([`load_workspace_or_default`], [`load_dock_or_default`]).

use crate::layout::dock::{PhotoDockTab, default_tree, reconcile_canvases};
use egui_tiles::Tree;
use ui_kit::layout::{WorkspaceState, load_workspace, save_workspace};
use uuid::Uuid;

/// Sérialise le workspace en JSON.
///
/// En attente du câblage disque (`preferences`) : phase suivante.
///
/// # Errors
/// Retourne l'erreur `serde_json` si la sérialisation échoue.
#[allow(dead_code)]
pub fn save_workspace_state(state: &WorkspaceState) -> Result<String, serde_json::Error> {
    save_workspace(state)
}

/// Restaure le workspace (erreur si JSON invalide ou version
/// incompatible — voir [`load_workspace_or_default`]).
///
/// En attente du câblage disque (`preferences`) : phase suivante.
///
/// # Errors
/// Retourne une erreur si le JSON est invalide ou refusé par ui-kit.
#[allow(dead_code)]
pub fn load_workspace_state(json: &str) -> Result<WorkspaceState, serde_json::Error> {
    load_workspace(json)
}

/// Restaure le workspace, ou le layout par défaut si le JSON est
/// absent ou invalide (le boot ne doit jamais échouer pour un
/// layout).
pub fn load_workspace_or_default(json: Option<&str>) -> WorkspaceState {
    json.and_then(|raw| load_workspace(raw).ok())
        .unwrap_or_default()
}

/// Sérialise la disposition des tuiles en JSON.
///
/// `egui_tiles` ne persiste que la structure (parts, visibilités),
/// pas de géométrie infinie : la sauvegarde est valide même avant
/// le premier layout.
///
/// En attente du câblage disque (`preferences`) : phase suivante.
///
/// # Errors
/// Retourne l'erreur `serde_json` si la sérialisation échoue.
#[allow(dead_code)]
pub fn save_tree_state(state: &Tree<PhotoDockTab>) -> Result<String, serde_json::Error> {
    serde_json::to_string(state)
}

/// Restaure la disposition des tuiles (erreur si JSON invalide —
/// voir [`load_tree_or_default`]).
///
/// En attente du câblage disque (`preferences`) : phase suivante.
///
/// # Errors
/// Retourne une erreur si le JSON est invalide.
#[allow(dead_code)]
pub fn load_tree_state(json: &str) -> Result<Tree<PhotoDockTab>, serde_json::Error> {
    serde_json::from_str(json)
}

/// Restaure la disposition des tuiles, ou le layout par défaut si le
/// JSON est absent ou invalide. Les ids de documents changent à
/// chaque lancement : les canevas restaurés sont réconciliés avec
/// `ids` (orphelins retirés, manquants ajoutés).
pub fn load_tree_or_default(json: Option<&str>, ids: &[Uuid]) -> Tree<PhotoDockTab> {
    let mut state = json
        .and_then(|raw| load_tree_state(raw).ok())
        .unwrap_or_else(|| default_tree(ids.iter().map(|id| PhotoDockTab::Canvas(*id)).collect()));
    reconcile_canvases(&mut state, ids);
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

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

//! Workspace JSON persistence (via `packages/preferences`).
//!
//! Apps serialize on quit and restore on boot:
//! ```rust,no_run
//! # use ui_kit::layout::{WorkspaceState, load_workspace, save_workspace};
//! let state = WorkspaceState::defaults();
//! let json = save_workspace(&state).expect("serialisation");
//! let restored = load_workspace(&json).expect("restauration");
//! assert_eq!(state, restored);
//! ```

use super::workspace_state::{WORKSPACE_VERSION, WorkspaceState};

/// Serializes the workspace to JSON.
///
/// # Errors
/// Returns the `serde_json` error when serialization fails.
pub fn save_workspace(state: &WorkspaceState) -> Result<String, serde_json::Error> {
    serde_json::to_string(state)
}

/// Restores the workspace from its JSON.
///
/// # Errors
/// Returns an error when the JSON is invalid or its version
/// mismatches [`WORKSPACE_VERSION`] (clean refusal; the app then
/// falls back to [`WorkspaceState::defaults`]).
pub fn load_workspace(json: &str) -> Result<WorkspaceState, serde_json::Error> {
    let state: WorkspaceState = serde_json::from_str(json)?;
    if state.version != WORKSPACE_VERSION {
        return Err(serde::de::Error::custom(format!(
            "version de workspace incompatible : {} (attendue {})",
            state.version, WORKSPACE_VERSION
        )));
    }
    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::PanelId;

    #[test]
    fn roundtrip_preserves_state() {
        let mut state = WorkspaceState::defaults();
        assert!(state.set_visible(PanelId::Layers, false));
        let json = save_workspace(&state).expect("serialisation");
        let restored = load_workspace(&json).expect("restauration");
        assert_eq!(state, restored);
    }

    #[test]
    fn invalid_json_is_rejected() {
        assert!(load_workspace("pas du json").is_err());
    }

    #[test]
    fn wrong_version_is_rejected() {
        let json = r#"{"version":999,"left":[],"right":[],"bottom":[]}"#;
        assert!(load_workspace(json).is_err());
    }

    #[test]
    fn missing_regions_default_to_empty() {
        let state: WorkspaceState =
            serde_json::from_str(r#"{"version":1}"#).expect("champs optionnels");
        assert!(state.left.is_empty());
    }
}

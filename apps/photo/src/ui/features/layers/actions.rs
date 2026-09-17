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

//! Conversion des actions du panneau Calques en [`PhotoAction`].
//!
//! Seule la sélection reste locale à l'app ; tout le reste est routé
//! au worker par `PhotoApp::handle_action`. Les ids sont déjà résolus
//! à l'émission (le panneau connaît la sélection).

use super::panel::LayerPanelAction;
use crate::commands::PhotoAction;

/// Convertit une action du panneau en action app.
pub fn layer_panel_action_to_photo(action: LayerPanelAction) -> PhotoAction {
    match action {
        LayerPanelAction::AddImage => PhotoAction::AddImageLayer,
        LayerPanelAction::AddEmpty => PhotoAction::AddEmptyLayer,
        LayerPanelAction::Duplicate => PhotoAction::DuplicateSelectedLayer,
        LayerPanelAction::OpenFilterMenu => PhotoAction::OpenFilterMenu,
        LayerPanelAction::AddMask => PhotoAction::AddMaskToSelected,
        LayerPanelAction::Delete => PhotoAction::DeleteSelectedLayer,
        LayerPanelAction::Select(id) => PhotoAction::SelectLayer(id),
        LayerPanelAction::RenameCommit { layer, name } => PhotoAction::RenameLayer { layer, name },
        LayerPanelAction::Reorder { from, to } => PhotoAction::ReorderLayers { from, to },
        LayerPanelAction::ToggleVisibility(id) => PhotoAction::ToggleLayerVisibility(id),
        LayerPanelAction::DeleteLayer(id) => PhotoAction::DeleteLayer(id),
        LayerPanelAction::DuplicateLayer(id) => PhotoAction::DuplicateLayer(id),
        LayerPanelAction::MoveFilter { layer, filter, up } => {
            PhotoAction::MoveFilter { layer, filter, up }
        }
        LayerPanelAction::MoveMask { owner, mask, up } => PhotoAction::MoveMask { owner, mask, up },
        LayerPanelAction::RemoveFilter { layer, filter } => {
            PhotoAction::RemoveFilter { layer, filter }
        }
        LayerPanelAction::RemoveMask { owner, mask } => PhotoAction::RemoveMask { owner, mask },
        LayerPanelAction::SetOpacity { layer, opacity } => {
            PhotoAction::SetOpacity { layer, opacity }
        }
        LayerPanelAction::SetBlendMode { layer, mode } => PhotoAction::SetBlendMode { layer, mode },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn every_panel_action_converts() {
        let id = Uuid::new_v4();
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::Select(id)),
            PhotoAction::SelectLayer(id)
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::Reorder { from: 0, to: 2 }),
            PhotoAction::ReorderLayers { from: 0, to: 2 }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::ToggleVisibility(id)),
            PhotoAction::ToggleLayerVisibility(id)
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::Delete),
            PhotoAction::DeleteSelectedLayer
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::DuplicateLayer(id)),
            PhotoAction::DuplicateLayer(id)
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::SetOpacity {
                layer: id,
                opacity: 42.0
            }),
            PhotoAction::SetOpacity {
                layer: id,
                opacity: 42.0
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::SetBlendMode {
                layer: id,
                mode: photo_engine::BlendMode::Screen,
            }),
            PhotoAction::SetBlendMode {
                layer: id,
                mode: photo_engine::BlendMode::Screen,
            }
        );
    }
}

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

//! Converting Layers panel actions into [`PhotoAction`].
//!
//! Only selection stays local to the app; everything else is routed
//! to the worker by `PhotoApp::handle_action`. Ids are already resolved
//! at emission (the panel knows the selection).

use super::panel::LayerPanelAction;
use crate::commands::PhotoAction;

/// Converts a panel action into an app action.
pub fn layer_panel_action_to_photo(action: LayerPanelAction) -> PhotoAction {
    match action {
        LayerPanelAction::AddImage => PhotoAction::AddImageLayer,
        LayerPanelAction::AddEmpty => PhotoAction::AddEmptyLayer,
        LayerPanelAction::Duplicate => PhotoAction::DuplicateSelectedLayer,
        LayerPanelAction::AddFilter { layer, filter_type } => {
            PhotoAction::AddFilter { layer, filter_type }
        }
        LayerPanelAction::AddMask => PhotoAction::AddMaskToSelected,
        LayerPanelAction::AddMaskToFilter(filter) => PhotoAction::AddMaskToFilter { filter },
        LayerPanelAction::Delete => PhotoAction::DeleteSelectedLayer,
        LayerPanelAction::Select(id) => PhotoAction::SelectLayer(id),
        LayerPanelAction::ToggleSelect(id) => PhotoAction::ToggleLayerSelection(id),
        LayerPanelAction::SelectAttachment { owner, id } => {
            PhotoAction::FocusAttachment { owner, id }
        }
        LayerPanelAction::ToggleFilter { layer, filter } => {
            PhotoAction::ToggleFilter { layer, filter }
        }
        LayerPanelAction::ToggleMask { owner, mask } => PhotoAction::ToggleMask { owner, mask },
        LayerPanelAction::RenameCommit { layer, name } => PhotoAction::RenameLayer { layer, name },
        LayerPanelAction::ReorderNodes {
            dragged,
            target,
            before,
        } => PhotoAction::ReorderNodes {
            dragged,
            target,
            before,
        },
        LayerPanelAction::MoveIntoParent { layer, parent } => {
            PhotoAction::MoveIntoParent { layer, parent }
        }
        LayerPanelAction::ToggleCollapsed(id) => PhotoAction::ToggleGroupCollapsed(id),
        LayerPanelAction::ToggleVisibility(id) => PhotoAction::ToggleLayerVisibility(id),
        LayerPanelAction::IsolateLayer(id) => PhotoAction::IsolateLayer(id),
        LayerPanelAction::DeleteLayer(id) => PhotoAction::DeleteLayer(id),
        LayerPanelAction::DuplicateLayer(id) => PhotoAction::DuplicateLayer(id),
        LayerPanelAction::FlipHorizontalSelected => PhotoAction::FlipHorizontalSelected,
        LayerPanelAction::FlipVerticalSelected => PhotoAction::FlipVerticalSelected,
        LayerPanelAction::RotateClockwiseSelected => PhotoAction::RotateClockwiseSelected,
        LayerPanelAction::RotateCounterclockwiseSelected => {
            PhotoAction::RotateCounterclockwiseSelected
        }
        LayerPanelAction::CropSelectedToDocument => PhotoAction::CropSelectedToDocument,
        LayerPanelAction::MoveFilterTo {
            filter,
            from,
            to,
            index,
        } => PhotoAction::MoveFilterTo {
            filter,
            from,
            to,
            index,
        },
        LayerPanelAction::MoveMaskTo {
            mask,
            from,
            to,
            index,
        } => PhotoAction::MoveMaskTo {
            mask,
            from,
            to,
            index,
        },
        LayerPanelAction::RemoveFilter { layer, filter } => {
            PhotoAction::RemoveFilter { layer, filter }
        }
        LayerPanelAction::DuplicateFilter { layer, filter } => {
            PhotoAction::DuplicateFilter { layer, filter }
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
            layer_panel_action_to_photo(LayerPanelAction::ReorderNodes {
                dragged: id,
                target: id,
                before: true,
            }),
            PhotoAction::ReorderNodes {
                dragged: id,
                target: id,
                before: true,
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::MoveIntoParent {
                layer: id,
                parent: id
            }),
            PhotoAction::MoveIntoParent {
                layer: id,
                parent: id
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::ToggleCollapsed(id)),
            PhotoAction::ToggleGroupCollapsed(id)
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
            layer_panel_action_to_photo(LayerPanelAction::DuplicateFilter {
                layer: id,
                filter: id
            }),
            PhotoAction::DuplicateFilter {
                layer: id,
                filter: id
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::AddFilter {
                layer: id,
                filter_type: String::from("blur")
            }),
            PhotoAction::AddFilter {
                layer: id,
                filter_type: String::from("blur")
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::SelectAttachment { owner: id, id }),
            PhotoAction::FocusAttachment { owner: id, id }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::ToggleFilter {
                layer: id,
                filter: id
            }),
            PhotoAction::ToggleFilter {
                layer: id,
                filter: id
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::ToggleMask {
                owner: id,
                mask: id
            }),
            PhotoAction::ToggleMask {
                owner: id,
                mask: id
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::FlipHorizontalSelected),
            PhotoAction::FlipHorizontalSelected
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::CropSelectedToDocument),
            PhotoAction::CropSelectedToDocument
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
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::MoveFilterTo {
                filter: id,
                from: id,
                to: id,
                index: 0,
            }),
            PhotoAction::MoveFilterTo {
                filter: id,
                from: id,
                to: id,
                index: 0,
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::MoveMaskTo {
                mask: id,
                from: id,
                to: id,
                index: 1,
            }),
            PhotoAction::MoveMaskTo {
                mask: id,
                from: id,
                to: id,
                index: 1,
            }
        );
        assert_eq!(
            layer_panel_action_to_photo(LayerPanelAction::AddMaskToFilter(id)),
            PhotoAction::AddMaskToFilter { filter: id }
        );
    }
}

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

//! Converting inspector actions into [`PhotoAction`].

use super::panel::InspectorAction;
use crate::commands::PhotoAction;

/// Converts an inspector action into an app action.
pub fn inspector_action_to_photo(action: InspectorAction) -> PhotoAction {
    match action {
        InspectorAction::ToggleAttachment {
            owner,
            id,
            is_filter,
        } => {
            if is_filter {
                PhotoAction::ToggleFilter {
                    layer: owner,
                    filter: id,
                }
            } else {
                PhotoAction::ToggleMask { owner, mask: id }
            }
        }
        InspectorAction::SetFilterParam {
            owner,
            id,
            key,
            value,
        } => PhotoAction::SetFilterParam {
            layer: owner,
            filter: id,
            key,
            value,
        },
        InspectorAction::SetFilterText {
            owner,
            id,
            key,
            value,
        } => PhotoAction::SetFilterText {
            layer: owner,
            filter: id,
            key,
            value,
        },
        InspectorAction::ApplyPreset { owner, id, preset } => PhotoAction::ApplyFilterPreset {
            layer: owner,
            filter: id,
            preset,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn every_inspector_action_converts() {
        let owner = Uuid::new_v4();
        let id = Uuid::new_v4();
        assert_eq!(
            inspector_action_to_photo(InspectorAction::ToggleAttachment {
                owner,
                id,
                is_filter: true
            }),
            PhotoAction::ToggleFilter {
                layer: owner,
                filter: id
            }
        );
        assert_eq!(
            inspector_action_to_photo(InspectorAction::ToggleAttachment {
                owner,
                id,
                is_filter: false
            }),
            PhotoAction::ToggleMask { owner, mask: id }
        );
        assert_eq!(
            inspector_action_to_photo(InspectorAction::SetFilterParam {
                owner,
                id,
                key: String::from("brightness"),
                value: 25.0
            }),
            PhotoAction::SetFilterParam {
                layer: owner,
                filter: id,
                key: String::from("brightness"),
                value: 25.0
            }
        );
        assert_eq!(
            inspector_action_to_photo(InspectorAction::SetFilterText {
                owner,
                id,
                key: String::from("points"),
                value: String::from("0,0;255,255")
            }),
            PhotoAction::SetFilterText {
                layer: owner,
                filter: id,
                key: String::from("points"),
                value: String::from("0,0;255,255")
            }
        );
        assert_eq!(
            inspector_action_to_photo(InspectorAction::ApplyPreset {
                owner,
                id,
                preset: String::from("hsl.vivid")
            }),
            PhotoAction::ApplyFilterPreset {
                layer: owner,
                filter: id,
                preset: String::from("hsl.vivid")
            }
        );
    }
}

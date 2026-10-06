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

//! Conversion des actions de l'inspecteur en [`PhotoAction`].

use super::panel::InspectorAction;
use crate::commands::PhotoAction;

/// Convertit une action de l'inspecteur en action app.
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
    }
}

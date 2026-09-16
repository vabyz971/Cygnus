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
        InspectorAction::ToggleVisibility(id) => PhotoAction::ToggleLayerVisibility(id),
        InspectorAction::SetOpacity { layer, opacity } => {
            PhotoAction::SetOpacity { layer, opacity }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn every_inspector_action_converts() {
        let id = Uuid::new_v4();
        assert_eq!(
            inspector_action_to_photo(InspectorAction::ToggleVisibility(id)),
            PhotoAction::ToggleLayerVisibility(id)
        );
        assert_eq!(
            inspector_action_to_photo(InspectorAction::SetOpacity {
                layer: id,
                opacity: 42.0
            }),
            PhotoAction::SetOpacity {
                layer: id,
                opacity: 42.0
            }
        );
    }
}

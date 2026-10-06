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

//! Conversion des actions du panneau Historique en [`PhotoAction`].

use super::panel::HistoryPanelAction;
use crate::commands::PhotoAction;

/// Convertit une action du panneau en action app.
pub fn history_panel_action_to_photo(action: HistoryPanelAction) -> PhotoAction {
    match action {
        HistoryPanelAction::Back(steps) => PhotoAction::HistoryBack(steps),
        HistoryPanelAction::Forward(steps) => PhotoAction::HistoryForward(steps),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_panel_action_converts() {
        assert_eq!(
            history_panel_action_to_photo(HistoryPanelAction::Back(2)),
            PhotoAction::HistoryBack(2)
        );
        assert_eq!(
            history_panel_action_to_photo(HistoryPanelAction::Forward(1)),
            PhotoAction::HistoryForward(1)
        );
    }
}

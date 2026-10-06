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

//! Fonctionnalité Historique : panneau position-indépendant.
//!
//! Liste des pas undo (plus ancien → plus récent), état actuel
//! surligné, pas redo (prochain en tête). Clic sur une rangée =
//! saut d'état ([`HistoryPanelAction`]), converti en
//! [`PhotoAction`](crate::commands::PhotoAction) (voir
//! [`super::actions`]). Style exclusivement ui-kit, aucune couleur
//! en dur.

pub mod actions;
pub mod panel;

pub use actions::history_panel_action_to_photo;
pub use panel::HistoryPanel;

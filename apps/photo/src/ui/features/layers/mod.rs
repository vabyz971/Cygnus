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

//! Fonctionnalité Calques : panneau position-indépendant.
//!
//! Types de snapshot (`types`), ligne HUD (`layer_item`), liste
//! réordonnable (`layer_list`), panneau (`panel`) et conversion vers
//! [`PhotoAction`](crate::commands::PhotoAction) (`actions`).
//! Seuls les symboles consommés hors feature sont réexportés ; les
//! échanges internes passent par des chemins explicites.

pub mod actions;
pub mod layer_item;
pub mod layer_list;
pub mod panel;
pub mod types;

pub use actions::layer_panel_action_to_photo;
pub use layer_item::LayerRenameState;
pub use panel::LayersPanel;
pub use types::{PhotoLayerInfo, snapshot_layers};

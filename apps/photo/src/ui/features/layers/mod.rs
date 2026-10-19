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

//! Layers feature: position-independent panel.
//!
//! Snapshot types (`types`), HUD row (`layer_item`), reorderable
//! list (`layer_list`), panel (`panel`) and conversion to
//! [`PhotoAction`](crate::commands::PhotoAction) (`actions`).
//! Only symbols consumed outside the feature are re-exported; internal
//! exchanges go through explicit paths.

pub mod actions;
pub mod layer_item;
pub mod layer_list;
pub mod panel;
pub mod types;

pub use actions::layer_panel_action_to_photo;
pub use layer_item::LayerRenameState;
pub use panel::LayersPanel;
pub use types::{
    AttachmentRef, LayerThumbView, LayerTreeSelection, PhotoLayerInfo, PhotoLayerThumb,
    PhotoSubLayerInfo, find_attachment_in, find_filter_in, find_layer_in, find_layer_in_mut,
    find_layer_of_attachment,
};

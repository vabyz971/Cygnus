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

//! Widgets egui MÉTIER de l'app Photo.
//!
//! Trois niveaux :
//!
//! - `features/` : fonctionnalités par domaine (calques, inspecteur),
//!   position-indépendantes, remontant des [`PhotoAction`](crate::commands::PhotoAction) ;
//! - `viewport/` : canvas + overlays autour du viewport générique ui-kit ;
//! - widgets plats (`menubar`, `modebar`, `toolbar`, `optionsbar`,
//!   `colorpanel`, `dialogs`) : contenus utilisés par `crate::layout`.
//!
//! Le pont moteur (`engine_bridge`, channels + worker) ne sait rien
//! d'egui au-delà des snapshots ; aucun widget n'y envoie directement.

pub mod colorpanel;
pub mod dialogs;
pub mod engine_bridge;
pub mod features;
pub mod menubar;
pub mod modebar;
pub mod optionsbar;
pub mod toolbar;
pub mod viewport;

pub use colorpanel::draw_color_panel;
pub use dialogs::{
    ExportDialogState, NewDocumentDialogState, draw_export_dialog, draw_help_dialog,
    draw_new_document_dialog,
};
pub use engine_bridge::{
    PhotoEngineCommand, PhotoEngineResponse, PreviewImage, spawn_photo_engine_worker,
};
pub use features::{
    inspector::{InspectorPanel, inspector_action_to_photo},
    layers::{LayerRenameState, LayersPanel, PhotoLayerInfo, layer_panel_action_to_photo},
};
pub use menubar::{MenuAvailability, PhotoMenuAction, draw_menu_bar};
pub use modebar::{PhotoEditMode, draw_photo_modebar};
pub use toolbar::draw_tool_rail;
pub use viewport::{
    PaintRequest, PhotoBrushSettings, PhotoCanvas, PhotoCanvasTool, draw_origin_marker,
    draw_selection_chip,
};

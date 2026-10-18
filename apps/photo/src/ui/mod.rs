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

//! Photo app DOMAIN egui widgets.
//!
//! Three levels:
//!
//! - `features/`: per-domain features (layers, inspector),
//!   position-independent, bubbling [`PhotoAction`](crate::commands::PhotoAction)s;
//! - `viewport/`: canvas + overlays around the generic cygnus-ui viewport;
//! - flat widgets (`menubar`, `modebar`, `toolbar`, `optionsbar`,
//!   `dialogs`): contents used by `crate::layout`.
//!
//! The engine bridge (`engine_bridge`, channels + worker) knows nothing
//! of egui beyond snapshots; no widget sends to it directly.

pub mod create_document;
pub mod dialogs;
pub mod drag_proxy;
pub mod engine_bridge;
pub mod features;
pub mod ink_overlay;
pub mod menubar;
pub mod modebar;
pub mod optionsbar;
/// Perf measurement probes (temporary diagnostics, tests only).
#[cfg(test)]
mod probes;
pub mod status;
pub mod toolbar;
pub mod transform_overlay;
pub mod viewport;

pub use create_document::{
    CreateDocumentDialogState, NewDocumentChoice, draw_create_document_dialog,
};
pub use dialogs::{ExportDialogState, draw_export_dialog, draw_help_dialog};
pub use engine_bridge::{
    NewVectorShape, PhotoEngineCommand, PhotoEngineResponse, PreviewImage,
    spawn_photo_engine_worker,
};
pub use features::{
    histogram::HistogramPanel,
    history::{HistoryPanel, history_panel_action_to_photo},
    inspector::{InspectorPanel, inspector_action_to_photo},
    layers::{
        AttachmentRef, LayerRenameState, LayerThumbView, LayersPanel, PhotoLayerInfo,
        PhotoLayerThumb, layer_panel_action_to_photo,
    },
};
pub use ink_overlay::{
    InkFrameGeom, InkFrameInput, clear_ink, draw_cached_ink, feed_ink_frame, ink_tool_active,
};
pub use menubar::{MenuAvailability, PhotoMenuAction, draw_menu_bar};
pub use modebar::{PhotoEditMode, draw_photo_modebar};
pub use toolbar::draw_tool_rail;
pub use transform_overlay::{
    NodeDrag, NodePoint, NodeRef, StabilityAction, StableView, TransformDrag, doc_to_screen,
    draw_node_overlay, draw_transform_overlay, frame_corners, hit_test_topmost, path_nodes,
    point_in_quad, screen_to_doc, stability_action,
};
pub use viewport::{
    CanvasMapping, PaintRequest, PhotoBrushSettings, PhotoCanvas, PhotoCanvasOutcome,
    PhotoCanvasTool, draw_brush_cursor, draw_document_bounds,
};

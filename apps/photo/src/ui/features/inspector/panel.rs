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

//! Inspector panel: focused attachment properties.
//!
//! Position-independent (the workspace decides placement and draws
//! the section header). Selecting a layer ALONE shows the empty hint
//! (no visibility/opacity/blend duplicates, which
//! live in the Layers panel); only a focused
//! attachment (live filter or mask, click on its tree row)
//! shows its settings section.
//! No worker send: everything bubbles as [`InspectorAction`].

use super::sections::draw_attachment_section;
use crate::commands::PhotoUiContext;
use crate::ui::PhotoLayerInfo;
use crate::ui::features::layers::types::{AttachmentRef, find_attachment_in};
use cygnus_ui::primitives::Text;
use uuid::Uuid;

/// Action emitted by the inspector (routed to the worker by the app, see
/// [`super::actions`]).
#[derive(Debug, Clone, PartialEq)]
pub enum InspectorAction {
    /// Enable/disable the focused attachment.
    ToggleAttachment {
        /// Owning layer.
        owner: Uuid,
        /// Targeted filter or mask.
        id: Uuid,
        /// True = live filter, false = mask.
        is_filter: bool,
    },
    /// Set a float parameter of the focused filter (sliders).
    SetFilterParam {
        /// Owning layer.
        owner: Uuid,
        /// Targeted filter.
        id: Uuid,
        /// Parameter key (e.g. `"brightness"`).
        key: String,
        /// New value.
        value: f32,
    },
    /// Set a text parameter of the focused filter (curve editor).
    SetFilterText {
        /// Owning layer.
        owner: Uuid,
        /// Targeted filter.
        id: Uuid,
        /// Parameter key (e.g. `"points"`).
        key: String,
        /// New value.
        value: String,
    },
    /// Set the focused mask's properties (absolute values, coalesced
    /// slider gesture worker-side).
    SetMaskParams {
        /// Mask owner.
        owner: Uuid,
        /// Targeted mask.
        id: Uuid,
        /// Density 0..=1.
        density: f32,
        /// Feather radius (mask pixels).
        feather: f32,
        /// Inverted coverage.
        inverted: bool,
    },
    /// Replace the selected pixel layer's styles (absolute values,
    /// coalesced slider gesture worker-side).
    SetLayerStyles {
        /// Target pixel layer.
        layer: Uuid,
        /// New styles.
        styles: photo_engine::document::styles::LayerStyles,
    },
    /// Install a built-in preset on the focused filter.
    ApplyPreset {
        /// Owning layer.
        owner: Uuid,
        /// Targeted filter.
        id: Uuid,
        /// Preset id.
        preset: String,
    },
}

/// Inspector panel (direct content, no chrome: the title is
/// carried by the dock tab).
pub struct InspectorPanel;

impl InspectorPanel {
    /// Draws the content and returns the actions.
    pub fn show(
        ui: &mut egui::Ui,
        ctx: &PhotoUiContext,
        selected: Option<&PhotoLayerInfo>,
        focused: Option<AttachmentRef>,
    ) -> Vec<InspectorAction> {
        let theme = ctx.shared.theme();
        let texts = cygnus_i18n::PhotoCatalog::new(ctx.shared.translator().language());
        let empty = |ui: &mut egui::Ui| {
            Text::caption(
                theme,
                texts.get(cygnus_i18n::PhotoTextKey::AdjustmentsEmpty),
            )
            .show(ui);
        };
        let Some(layer) = selected else {
            empty(ui);
            return Vec::new();
        };
        // Lone layer: empty panel (layer settings live
        // in the Layers panel). Only a focused attachment shows
        // its section: owner = selection, or one of its filters
        // (filter-owned masks).
        let Some(focus) = focused else {
            // Pixel layer alone: its styles; anything else: the hint.
            return match layer.styles {
                Some(styles) => super::sections::draw_styles_section(ui, ctx, layer.id, styles),
                None => {
                    empty(ui);
                    Vec::new()
                }
            };
        };
        let owner_is_layer = focus.owner == layer.id;
        let owner_is_filter = crate::ui::features::layers::types::find_filter_in(
            std::slice::from_ref(layer),
            focus.owner,
        )
        .is_some();
        if !owner_is_layer && !owner_is_filter {
            return Vec::new();
        }
        let Some((sub, _)) = find_attachment_in(std::slice::from_ref(layer), focus.owner, focus.id)
        else {
            return Vec::new();
        };
        draw_attachment_section(ui, ctx, focus, sub)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photo_engine::{Document, LayerNode, PixelLayer};
    use std::sync::Arc;

    fn fixture_layer() -> PhotoLayerInfo {
        let mut doc = Document::new(8, 8);
        let image = Arc::new(image::DynamicImage::new_rgba8(4, 4));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", image)));
        crate::ui::features::layers::types::snapshot_layers(&doc)
            .pop()
            .expect("un calque")
    }

    #[test]
    fn inspector_renders_with_and_without_selection() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let layer = fixture_layer();
        let mut with_selection = Vec::new();
        let mut without_selection = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                with_selection = InspectorPanel::show(ui, &photo_ctx, Some(&layer), None);
                without_selection = InspectorPanel::show(ui, &photo_ctx, None, None);
            });
        })
        .drop_without_applying_deltas();
        assert!(with_selection.is_empty(), "aucun clic sans interaction");
        assert!(without_selection.is_empty());
    }
}

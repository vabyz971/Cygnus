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
//! Position-independent (the workspace decides placement, title
//! carried by the dock tab). Selecting a layer ALONE leaves the
//! panel empty (no visibility/opacity/blend duplicates, which
//! live in the Layers panel); only a focused
//! attachment (live filter or mask, click on its tree row)
//! shows its settings section. `None` = explicit empty state.
//! No worker send: everything bubbles as [`InspectorAction`].

use super::sections::draw_attachment_section;
use crate::commands::PhotoUiContext;
use crate::ui::PhotoLayerInfo;
use crate::ui::features::layers::types::{AttachmentRef, find_attachment_in};
use ui_kit::primitives::Text;
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
        let texts = crate::i18n::PhotoCatalog::new(ctx.shared.translator().language());
        let Some(layer) = selected else {
            Text::body(theme, texts.get(crate::i18n::PhotoTextKey::NoLayerSelected)).show(ui);
            return Vec::new();
        };
        // Lone layer: empty panel (layer settings live
        // in the Layers panel). Only a focused attachment
        // (owner = selection, existing id) shows its section.
        let Some(focus) = focused else {
            return Vec::new();
        };
        if focus.owner != layer.id {
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
        ui_kit::theme::setup_fonts(&ctx);
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

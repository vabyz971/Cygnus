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

//! Panneau Inspecteur : propriétés du calque sélectionné.
//!
//! Position-indépendant (le workspace décide du placement, titre
//! porté par l'onglet dock). En-tête (nom, type, compteurs) +
//! sections. `None` = état vide explicite. Aucun envoi worker :
//! tout remonte en [`InspectorAction`].

use super::sections::draw_appearance_section;
use crate::commands::PhotoUiContext;
use crate::ui::PhotoLayerInfo;
use ui_kit::primitives::Text;
use uuid::Uuid;

/// Action émise par l'inspecteur (routée au worker par l'app, voir
/// [`super::actions`]).
#[derive(Debug, Clone, PartialEq)]
pub enum InspectorAction {
    /// Basculer la visibilité.
    ToggleVisibility(Uuid),
    /// Régler l'opacité (unités moteur 0..=100).
    SetOpacity { layer: Uuid, opacity: f32 },
}

/// Panneau Inspecteur (contenu direct, sans chrome : le titre est
/// porté par l'onglet dock).
pub struct InspectorPanel;

impl InspectorPanel {
    /// Dessine le contenu et retourne les actions.
    pub fn show(
        ui: &mut egui::Ui,
        ctx: &PhotoUiContext,
        selected: Option<&PhotoLayerInfo>,
    ) -> Vec<InspectorAction> {
        let theme = ctx.shared.theme();
        let Some(layer) = selected else {
            Text::body(theme, "Aucun calque selectionne").show(ui);
            return Vec::new();
        };
        Text::heading(theme, &layer.name).show(ui);
        Text::body(
            theme,
            &format!(
                "{} · Filtres : {} · Masques : {}",
                layer.kind.icon_label(),
                layer.filters.len(),
                layer.masks.len()
            ),
        )
        .show(ui);
        draw_appearance_section(ui, ctx, layer)
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
                with_selection = InspectorPanel::show(ui, &photo_ctx, Some(&layer));
                without_selection = InspectorPanel::show(ui, &photo_ctx, None);
            });
        })
        .drop_without_applying_deltas();
        assert!(with_selection.is_empty(), "aucun clic sans interaction");
        assert!(without_selection.is_empty());
    }
}

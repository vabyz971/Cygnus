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

//! Section Apparence : visibilité + opacité du calque.
//!
//! Construite uniquement avec ui-kit ([`Section`](ui_kit::containers::Section),
//! [`Toggle`](ui_kit::components::Toggle),
//! [`Slider`](ui_kit::components::Slider)). Les gestes continus
//! (slider) sont coalescés côté worker (`push_coalesced`) : l'UI
//! émet sans modération, le moteur snapshotte en PRÉ-mutation.

use super::super::InspectorAction;
use crate::commands::PhotoUiContext;
use crate::ui::features::layers::PhotoLayerInfo;
use ui_kit::components::{Slider, Toggle};
use ui_kit::containers::Section;

/// Dessine la section et retourne les actions (routées au worker).
pub fn draw_appearance_section(
    ui: &mut egui::Ui,
    ctx: &PhotoUiContext,
    layer: &PhotoLayerInfo,
) -> Vec<InspectorAction> {
    let theme = ctx.shared.theme();
    let mut actions = Vec::new();
    Section::new("Apparence").show(ui, theme, |ui| {
        let mut visible = layer.visible;
        Toggle::new("Visible").show(ui, theme, &mut visible);
        if visible != layer.visible {
            actions.push(InspectorAction::ToggleVisibility(layer.id));
        }
        // Unités moteur 0..=100 (comme l'ancien slider).
        let mut opacity = layer.opacity;
        Slider::new("Opacite", 0.0..=100.0).show(ui, theme, &mut opacity);
        if opacity != layer.opacity {
            actions.push(InspectorAction::SetOpacity {
                layer: layer.id,
                opacity,
            });
        }
    });
    actions
}

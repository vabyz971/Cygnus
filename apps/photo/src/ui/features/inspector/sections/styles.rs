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

//! Layer styles section: drop shadow and outer glow of the selected
//! pixel layer (shown when no attachment is focused). Any change sends
//! the whole style set (absolute values, coalesced worker-side).

use super::super::InspectorAction;
use crate::commands::PhotoUiContext;
use cygnus_i18n::PhotoTextKey;
use cygnus_ui::components::{Slider, SliderValue, Toggle};
use cygnus_ui::primitives::Text;
use photo_engine::document::styles::{LayerStyles, MAX_STYLE_SIZE};
use uuid::Uuid;

/// Color swatch + picker of a style.
fn color_row(
    ui: &mut egui::Ui,
    theme: &cygnus_ui::theme::CygnusTheme,
    label: &str,
    color: &mut [u8; 3],
) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label)
                .size(theme.typography.body_size)
                .color(theme.colors.fg_secondary),
        );
        egui::color_picker::color_edit_button_srgb(ui, color);
    });
}

/// Draws the section for `layer` (current `styles`) and returns the
/// actions.
pub fn draw_styles_section(
    ui: &mut egui::Ui,
    ctx: &PhotoUiContext,
    layer: Uuid,
    styles: LayerStyles,
) -> Vec<InspectorAction> {
    let theme = ctx.shared.theme();
    let texts = cygnus_i18n::PhotoCatalog::new(ctx.shared.translator().language());
    let mut next = styles;
    Text::body(theme, texts.get(PhotoTextKey::LayerStylesTitle)).show(ui);
    ui.add_space(theme.spacing.xs);

    let shadow = &mut next.drop_shadow;
    ui.horizontal(|ui| {
        Toggle::new(texts.get(PhotoTextKey::DropShadowLabel)).show(ui, theme, &mut shadow.enabled);
    });
    if shadow.enabled {
        Slider::new(texts.get(PhotoTextKey::OpacityLabel), 0.0..=1.0)
            .stacked()
            .value(SliderValue::Percent)
            .show(ui, theme, &mut shadow.opacity);
        Slider::new(texts.get(PhotoTextKey::AngleLabel), 0.0..=360.0)
            .stacked()
            .show(ui, theme, &mut shadow.angle);
        Slider::new(texts.get(PhotoTextKey::DistanceLabel), 0.0..=MAX_STYLE_SIZE)
            .stacked()
            .show(ui, theme, &mut shadow.distance);
        Slider::new(texts.get(PhotoTextKey::SizeLabel), 0.0..=MAX_STYLE_SIZE)
            .stacked()
            .show(ui, theme, &mut shadow.size);
        color_row(
            ui,
            theme,
            texts.get(PhotoTextKey::ColorLabel),
            &mut shadow.color,
        );
    }
    ui.add_space(theme.spacing.sm);

    let glow = &mut next.outer_glow;
    ui.horizontal(|ui| {
        Toggle::new(texts.get(PhotoTextKey::OuterGlowLabel)).show(ui, theme, &mut glow.enabled);
    });
    if glow.enabled {
        Slider::new(texts.get(PhotoTextKey::OpacityLabel), 0.0..=1.0)
            .stacked()
            .value(SliderValue::Percent)
            .show(ui, theme, &mut glow.opacity);
        Slider::new(texts.get(PhotoTextKey::SizeLabel), 0.0..=MAX_STYLE_SIZE)
            .stacked()
            .show(ui, theme, &mut glow.size);
        color_row(
            ui,
            theme,
            texts.get(PhotoTextKey::ColorLabel),
            &mut glow.color,
        );
    }

    if next != styles {
        vec![InspectorAction::SetLayerStyles {
            layer,
            styles: next,
        }]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn section_renders_idle_for_both_states() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let mut on = LayerStyles::default();
        on.drop_shadow.enabled = true;
        on.outer_glow.enabled = true;
        let mut actions = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                actions.extend(draw_styles_section(ui, &photo_ctx, Uuid::new_v4(), on));
                actions.extend(draw_styles_section(
                    ui,
                    &photo_ctx,
                    Uuid::new_v4(),
                    LayerStyles::default(),
                ));
            });
        })
        .drop_without_applying_deltas();
        assert!(actions.is_empty(), "no change without interaction");
    }
}

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

//! Attachment section: the layer tree's secondary selection
//! (live filter or mask focused by click). Name, type,
//! on/off switch (toggles without losing settings or pixels) and,
//! for filters, setting sliders (brightness, contrast,
//! radius…) routed to the worker as a coalesced gesture.

use super::super::InspectorAction;
use crate::commands::PhotoUiContext;
use crate::i18n::PhotoTextKey;
use crate::ui::features::layers::types::{AttachmentRef, PhotoSubLayerInfo};
use ui_kit::components::{Slider, Toggle};
use ui_kit::primitives::{Text, divider};

/// Range of a float filter parameter, by registry key.
fn param_range(key: &str) -> std::ops::RangeInclusive<f32> {
    match key {
        "brightness" | "contrast" => -100.0..=100.0,
        "radius" => 0.0..=50.0,
        "saturation" => 0.0..=3.0,
        "hue" => -180.0..=180.0,
        _ => -100.0..=100.0,
    }
}

/// Parameter label (known registry keys, fallback = raw key).
fn param_label(key: &str, texts: crate::i18n::PhotoCatalog) -> String {
    match key {
        "brightness" => texts.get(PhotoTextKey::FilterParamBrightness).to_owned(),
        "contrast" => texts.get(PhotoTextKey::FilterParamContrast).to_owned(),
        "radius" => texts.get(PhotoTextKey::FilterParamRadius).to_owned(),
        "saturation" => texts.get(PhotoTextKey::FilterParamSaturation).to_owned(),
        "hue" => texts.get(PhotoTextKey::FilterParamHue).to_owned(),
        _ => key.to_owned(),
    }
}

/// Draws the section and returns the actions (routed to the worker).
pub fn draw_attachment_section(
    ui: &mut egui::Ui,
    ctx: &PhotoUiContext,
    focus: AttachmentRef,
    sub: &PhotoSubLayerInfo,
) -> Vec<InspectorAction> {
    let theme = ctx.shared.theme();
    let texts = crate::i18n::PhotoCatalog::new(ctx.shared.translator().language());
    let mut actions = Vec::new();
    divider(ui, theme);
    Text::heading(theme, &sub.name).show(ui);
    Text::body(
        theme,
        if sub.is_filter {
            texts.get(crate::i18n::PhotoTextKey::FilterLiveLabel)
        } else {
            texts.get(crate::i18n::PhotoTextKey::MaskLabel)
        },
    )
    .show(ui);
    let mut enabled = sub.enabled;
    Toggle::new(if sub.is_filter {
        texts.get(crate::i18n::PhotoTextKey::FilterActive)
    } else {
        texts.get(crate::i18n::PhotoTextKey::MaskActive)
    })
    .show(ui, theme, &mut enabled);
    if enabled != sub.enabled {
        actions.push(InspectorAction::ToggleAttachment {
            owner: focus.owner,
            id: focus.id,
            is_filter: sub.is_filter,
        });
    }
    // Filter parameter sliders (stable order: sorted keys).
    if sub.is_filter {
        let mut keys: Vec<&String> = sub.params.keys().collect();
        keys.sort();
        for key in keys {
            let current = sub.params.get(key).copied().unwrap_or(0.0);
            let mut value = current;
            Slider::new(&param_label(key, texts), param_range(key)).show(ui, theme, &mut value);
            if value != current {
                actions.push(InspectorAction::SetFilterParam {
                    owner: focus.owner,
                    id: focus.id,
                    key: key.clone(),
                    value,
                });
            }
        }
    }
    actions
}

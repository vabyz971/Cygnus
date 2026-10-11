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

//! Section Pièce jointe : la sélection secondaire de l'arbre des
//! calques (filtre live ou masque focalisé au clic). Nom, type,
//! interrupteur on/off (bascule sans perdre réglages ni pixels) et,
//! pour les filtres, curseurs de réglages (luminosité, contraste,
//! rayon…) routés au worker en geste coalescé.

use super::super::InspectorAction;
use crate::commands::PhotoUiContext;
use crate::i18n::PhotoTextKey;
use crate::ui::features::layers::types::{AttachmentRef, PhotoSubLayerInfo};
use ui_kit::components::{Slider, Toggle};
use ui_kit::primitives::{Text, divider};

/// Plage d'un paramètre flottant de filtre, par clé du registre.
fn param_range(key: &str) -> std::ops::RangeInclusive<f32> {
    match key {
        "brightness" | "contrast" => -100.0..=100.0,
        "radius" => 0.0..=50.0,
        "saturation" => 0.0..=3.0,
        "hue" => -180.0..=180.0,
        _ => -100.0..=100.0,
    }
}

/// Libellé d'un paramètre (clés du registre connues, repli = clé brute).
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

/// Dessine la section et retourne les actions (routées au worker).
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
    // Curseurs des paramètres du filtre (ordre stable : clés triées).
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

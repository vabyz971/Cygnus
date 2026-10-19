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
//! for filters, a preset picker (O005), setting sliders (brightness,
//! levels, HSL…) and the curve editor, routed to the worker as
//! coalesced gestures.

use super::super::InspectorAction;
use super::curve_editor::curve_editor;
use crate::commands::PhotoUiContext;
use crate::ui::features::layers::types::{AttachmentRef, PhotoSubLayerInfo};
use cygnus_i18n::PhotoTextKey;
use cygnus_ui::components::{Select, Slider, Toggle};
use cygnus_ui::icons::{Icon, IconRegistry};
use cygnus_ui::primitives::Text;
use cygnus_ui::theme::typography::caption_text;
use photo_engine::nodes::{curves, hsl, levels};

/// Range of a float filter parameter, by effect type and registry key.
fn param_range(type_id: &str, key: &str) -> std::ops::RangeInclusive<f32> {
    match (type_id, key) {
        (levels::TYPE_ID, "gamma") => 0.1..=3.0,
        (levels::TYPE_ID, _) => 0.0..=255.0,
        (hsl::TYPE_ID, "hue") => -180.0..=180.0,
        (hsl::TYPE_ID, _) => -100.0..=100.0,
        (_, "brightness" | "contrast") => -100.0..=100.0,
        (_, "radius") => 0.0..=50.0,
        (_, "saturation") => 0.0..=3.0,
        (_, "hue") => -180.0..=180.0,
        _ => -100.0..=100.0,
    }
}

/// Slider order: the natural reading order for adjustments, sorted
/// keys otherwise (stable).
fn param_order<'a>(type_id: &str, params: impl Iterator<Item = &'a String>) -> Vec<&'a String> {
    let natural: &[&str] = match type_id {
        levels::TYPE_ID => &["in_black", "in_white", "gamma", "out_black", "out_white"],
        hsl::TYPE_ID => &["hue", "saturation", "lightness"],
        _ => &[],
    };
    let mut keys: Vec<&String> = params.collect();
    keys.sort_by_key(|k| {
        (
            natural.iter().position(|n| n == k).unwrap_or(natural.len()),
            (*k).clone(),
        )
    });
    keys
}

/// Parameter label (known registry keys, fallback = raw key).
fn param_label(key: &str, texts: cygnus_i18n::PhotoCatalog) -> String {
    match key {
        "brightness" => texts.get(PhotoTextKey::FilterParamBrightness).to_owned(),
        "contrast" => texts.get(PhotoTextKey::FilterParamContrast).to_owned(),
        "radius" => texts.get(PhotoTextKey::FilterParamRadius).to_owned(),
        "saturation" => texts.get(PhotoTextKey::FilterParamSaturation).to_owned(),
        "hue" => texts.get(PhotoTextKey::FilterParamHue).to_owned(),
        "lightness" => texts.get(PhotoTextKey::FilterParamLightness).to_owned(),
        "in_black" => texts.get(PhotoTextKey::FilterParamInBlack).to_owned(),
        "in_white" => texts.get(PhotoTextKey::FilterParamInWhite).to_owned(),
        "gamma" => texts.get(PhotoTextKey::FilterParamGamma).to_owned(),
        "out_black" => texts.get(PhotoTextKey::FilterParamOutBlack).to_owned(),
        "out_white" => texts.get(PhotoTextKey::FilterParamOutWhite).to_owned(),
        _ => key.to_owned(),
    }
}

/// Translated preset name (stable engine id → catalog key).
fn preset_label(id: &str, texts: cygnus_i18n::PhotoCatalog) -> &'static str {
    let key = match id {
        "levels.high_contrast" => PhotoTextKey::PresetLevelsHighContrast,
        "levels.brighten" => PhotoTextKey::PresetLevelsBrighten,
        "levels.darken" => PhotoTextKey::PresetLevelsDarken,
        "levels.faded" => PhotoTextKey::PresetLevelsFaded,
        "curves.soft_contrast" => PhotoTextKey::PresetCurvesSoftContrast,
        "curves.strong_contrast" => PhotoTextKey::PresetCurvesStrongContrast,
        "curves.brighten" => PhotoTextKey::PresetCurvesBrighten,
        "curves.darken" => PhotoTextKey::PresetCurvesDarken,
        "curves.matte" => PhotoTextKey::PresetCurvesMatte,
        "curves.invert" => PhotoTextKey::PresetCurvesInvert,
        "hsl.black_and_white" => PhotoTextKey::PresetHslBlackAndWhite,
        "hsl.muted" => PhotoTextKey::PresetHslMuted,
        "hsl.vivid" => PhotoTextKey::PresetHslVivid,
        "hsl.brighten" => PhotoTextKey::PresetHslBrighten,
        "hsl.complementary" => PhotoTextKey::PresetHslComplementary,
        _ => PhotoTextKey::PresetDefault,
    };
    texts.get(key)
}

/// True when the filter's current values equal the preset's (floats
/// within a slider-noise tolerance, curve points compared parsed).
fn matches_preset(sub: &PhotoSubLayerInfo, preset: &photo_engine::FilterPreset) -> bool {
    preset.params.iter().all(|(key, value)| match value {
        datatypes::ParamValue::Float(v) => sub
            .params
            .get(key)
            .is_some_and(|current| (current - v).abs() < 1e-3),
        datatypes::ParamValue::Text(t) => sub
            .text_params
            .get(key)
            .is_some_and(|current| curves::parse_points(current) == curves::parse_points(t)),
        _ => true,
    })
}

/// Preset picker: "Custom" leads the list when the settings match no
/// preset; picking an entry emits [`InspectorAction::ApplyPreset`].
fn draw_presets(
    ui: &mut egui::Ui,
    theme: &cygnus_ui::theme::CygnusTheme,
    texts: cygnus_i18n::PhotoCatalog,
    focus: AttachmentRef,
    sub: &PhotoSubLayerInfo,
    actions: &mut Vec<InspectorAction>,
) {
    let presets = photo_engine::presets_for(&sub.filter_type);
    if presets.len() < 2 {
        return;
    }
    let current = presets.iter().position(|p| matches_preset(sub, p));
    let offset = usize::from(current.is_none());
    let mut labels: Vec<&str> = Vec::with_capacity(presets.len() + offset);
    if current.is_none() {
        labels.push(texts.get(PhotoTextKey::PresetCustom));
    }
    labels.extend(presets.iter().map(|p| preset_label(p.id, texts)));
    let shown = current.map_or(0, |i| i + offset);
    let mut picked = shown;
    Select::new(texts.get(PhotoTextKey::PresetLabel), &labels).show(ui, theme, &mut picked);
    if picked != shown
        && let Some(preset) = picked.checked_sub(offset).and_then(|i| presets.get(i))
    {
        actions.push(InspectorAction::ApplyPreset {
            owner: focus.owner,
            id: focus.id,
            preset: preset.id.to_owned(),
        });
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
    let texts = cygnus_i18n::PhotoCatalog::new(ctx.shared.translator().language());
    let mut actions = Vec::new();
    // Title row: kind glyph + name, enable switch on the right.
    let icon = if !sub.is_filter {
        Icon::Mask
    } else if sub.filter_type == curves::TYPE_ID {
        Icon::Curve
    } else {
        Icon::Tune
    };
    let mut enabled = sub.enabled;
    ui.horizontal(|ui| {
        ui.label(IconRegistry::new().colored(
            icon,
            theme.typography.icon_size,
            theme.colors.accent,
        ));
        ui.label(
            egui::RichText::new(&sub.name)
                .size(theme.typography.body_size)
                .color(theme.colors.fg_strong),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            Toggle::new("")
                .show(ui, theme, &mut enabled)
                .on_hover_text(if sub.is_filter {
                    texts.get(PhotoTextKey::FilterActive)
                } else {
                    texts.get(PhotoTextKey::MaskActive)
                });
            ui.label(caption_text(
                theme,
                if sub.is_filter {
                    texts.get(PhotoTextKey::FilterLiveLabel)
                } else {
                    texts.get(PhotoTextKey::MaskLabel)
                },
            ));
        });
    });
    if enabled != sub.enabled {
        actions.push(InspectorAction::ToggleAttachment {
            owner: focus.owner,
            id: focus.id,
            is_filter: sub.is_filter,
        });
    }
    if sub.is_filter {
        draw_presets(ui, theme, texts, focus, sub, &mut actions);
        // Curve editor (text param) above the sliders.
        if sub.filter_type == curves::TYPE_ID {
            let text = sub
                .text_params
                .get(curves::POINTS_KEY)
                .map_or(curves::IDENTITY, String::as_str);
            if let Some(value) = curve_editor(ui, theme, focus.id, text) {
                actions.push(InspectorAction::SetFilterText {
                    owner: focus.owner,
                    id: focus.id,
                    key: curves::POINTS_KEY.to_owned(),
                    value,
                });
            }
            Text::caption(theme, texts.get(PhotoTextKey::CurveEditorHint)).show(ui);
        }
        // Float sliders (natural order for adjustments).
        for key in param_order(&sub.filter_type, sub.params.keys()) {
            let current = sub.params.get(key).copied().unwrap_or(0.0);
            let mut value = current;
            Slider::new(&param_label(key, texts), param_range(&sub.filter_type, key))
                .stacked()
                .show(ui, theme, &mut value);
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

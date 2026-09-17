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

//! Installation des polices et application du thème Cygnus sur egui.
//!
//! À appeler UNE SEULE FOIS au démarrage de chaque app, dans cet ordre :
//! 1. [`setup_fonts`] : police UI (Hanken Grotesk) + police d'icônes.
//! 2. [`apply_cygnus_theme`] : couleurs, espacements et rayons.

use super::CygnusTheme;

/// Charge la police UI (Hanken Grotesk) PUIS la police d'icônes Material
/// Symbols via `egui_material_icons`.
///
/// À appeler une seule fois au boot de chaque app, AVANT
/// [`apply_cygnus_theme`].
pub fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();

    // 1. Police UI Hanken Grotesk (versionnée dans assets/fonts/).
    fonts.font_data.insert(
        "hanken_grotesk".to_owned(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../../../../assets/fonts/HankenGrotesk-Regular.ttf"
        ))),
    );
    fonts
        .families
        .entry(egui::FontFamily::Proportional)
        .or_default()
        .insert(0, "hanken_grotesk".to_owned());

    ctx.set_fonts(fonts);

    // 2. Police d'icônes Material Symbols via la lib dédiée
    //    (jamais de codepoints gérés à la main).
    egui_material_icons::initialize(ctx);
}

/// Applique le thème Cygnus au contexte egui.
///
/// À appeler UNE SEULE FOIS au démarrage de chaque app, après
/// [`setup_fonts`].
pub fn apply_cygnus_theme(ctx: &egui::Context) {
    let theme = CygnusTheme::dark();
    ctx.set_theme(egui::Theme::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        style.visuals = egui::Visuals {
            dark_mode: true,
            override_text_color: Some(theme.colors.fg_primary),
            window_fill: theme.colors.bg_secondary,
            panel_fill: theme.colors.bg_primary,
            extreme_bg_color: theme.colors.bg_tertiary,
            ..Default::default()
        };
        style.visuals.widgets.noninteractive.bg_fill = theme.colors.bg_secondary;
        style.visuals.widgets.noninteractive.fg_stroke =
            egui::Stroke::new(1.0, theme.colors.fg_secondary);
        style.visuals.selection.bg_fill = theme.colors.item_selected;
        style.spacing.item_spacing = egui::vec2(theme.spacing.sm, theme.spacing.sm);
        style.spacing.button_padding = egui::vec2(theme.spacing.md, theme.spacing.sm);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn theme_applies_without_panic() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        apply_cygnus_theme(&ctx);
        assert!(ctx.style_of(egui::Theme::Dark).visuals.dark_mode);
    }
}

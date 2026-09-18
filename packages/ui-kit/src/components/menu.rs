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

//! Style partagé des menus et popups contextuels (façon rerun).
//!
//! Lignes compactes, marges resserrées, coins du thème : à appliquer
//! via `egui::Popup::context_menu(...).style(menu_style(theme))`.
//! Une variante `menu_bar_style` existe pour la barre de menus haute
//! (coins carrés, pas d'arrondi sur les boutons de titre).
//! Les lignes elles-mêmes utilisent [`menu_row`](super::button::menu_row).
//! 100 % tokens, aucune valeur en dur.

use crate::theme::CygnusTheme;

/// Style des popups de menu, dérivé du thème.
pub fn menu_style(theme: &CygnusTheme) -> egui::style::StyleModifier {
    let row_height = theme.sizes.button_sm;
    let margin = theme.spacing.xs;
    let padding_x = theme.spacing.xs;
    let corner = theme.radius.sm;
    egui::style::StyleModifier::new(move |style| {
        egui::containers::menu::menu_style(style);
        style.spacing.interact_size.y = row_height;
        style.spacing.menu_margin = margin.into();
        style.spacing.button_padding.x = padding_x;
        style.spacing.item_spacing.y = 0.0;
        for visual in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.open,
            &mut style.visuals.widgets.noninteractive,
        ] {
            visual.expansion = 0.0;
            visual.corner_radius = corner.into();
        }
    })
}

/// Style de la barre de menus haute
pub fn menu_bar_style(theme: &CygnusTheme) -> egui::style::StyleModifier {
    let row_height = theme.sizes.button_sm;
    let margin = theme.spacing.xs;
    let padding_x = theme.spacing.xs;
    let corner = theme.radius.sm;
    egui::style::StyleModifier::new(move |style| {
        style.spacing.interact_size.y = row_height;
        style.spacing.menu_margin = margin.into();
        style.spacing.button_padding.x = padding_x;
        style.spacing.item_spacing.y = 0.0;
        for visual in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.active,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.open,
            &mut style.visuals.widgets.noninteractive,
        ] {
            visual.expansion = 0.0;
            visual.corner_radius = corner.into();
        }
    })
}

/// Item de menu : bouton fantôme pleine largeur ; `true` si cliqué
/// (et menu refermé).
pub fn menu_item(ui: &mut egui::Ui, theme: &CygnusTheme, label: &str) -> bool {
    use crate::components::Button;
    use crate::components::ButtonSize;
    use crate::components::ButtonVariant;
    let clicked = Button::new(label)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Small)
        .show(ui, theme)
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Item de menu désactivable (remplace `add_enabled_ui` + bouton brut).
pub fn menu_item_enabled(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    label: &str,
    enabled: bool,
) -> bool {
    use crate::components::Button;
    use crate::components::ButtonSize;
    use crate::components::ButtonVariant;
    let clicked = Button::new(label)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Medium)
        .enabled(enabled)
        .show(ui, theme)
        .clicked();
    if clicked {
        ui.close();
    }
    clicked
}

/// Lignes de menu : voir [`menu_row`](super::button::menu_row),
/// réexporté ici pour un import unique.
pub use super::button::menu_row;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_style_comes_from_theme_tokens() {
        let theme = CygnusTheme::dark();
        let mut style = egui::Style::default();
        menu_style(&theme).apply(&mut style);
        assert_eq!(style.spacing.interact_size.y, theme.sizes.button_md);
        assert_eq!(style.spacing.item_spacing.y, 0.0);
        assert_eq!(
            style.visuals.widgets.hovered.corner_radius,
            theme.radius.sm.into()
        );
        // Base egui conservée : pas de contour sur les lignes.
        assert_eq!(style.visuals.widgets.inactive.bg_stroke, egui::Stroke::NONE);
    }

    #[test]
    fn reexported_menu_row_renders_idle() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        let mut clicked = true;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                clicked = menu_row(ui, &theme, "Dupliquer");
            });
        })
        .drop_without_applying_deltas();
        assert!(!clicked, "aucun clic sans interaction");
    }
}

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

//! Theme-calibrated button (variants + sizes).
//!
//! # Example
//! ```rust,no_run
//! # use ui_kit::components::{Button, ButtonVariant};
//! # use ui_kit::theme::CygnusTheme;
//! # egui::__run_test_ui(|ui| {
//! # let theme = CygnusTheme::dark();
//! if Button::new("Save")
//!     .variant(ButtonVariant::Primary)
//!     .show(ui, &theme)
//!     .clicked()
//! {
//!     // action triggered via UiCommand on the app side
//! }
//! # });
//! ```

use crate::theme::CygnusTheme;

/// Visual variant (no specialized types: one enum is enough).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonVariant {
    /// Primary action (accent).
    Primary,
    /// Secondary action (default).
    #[default]
    Secondary,
    /// Ghost button (transparent).
    Ghost,
    /// Destructive action (error).
    Danger,
}

/// Button size (heights in `theme.sizes`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    /// Small (24px).
    Small,
    /// Standard (32px, default).
    #[default]
    Medium,
    /// Large (40px).
    Large,
}

/// Button via builder API.
#[derive(Debug, Clone)]
pub struct Button<'a> {
    label: &'a str,
    variant: ButtonVariant,
    size: ButtonSize,
    enabled: bool,
}

impl<'a> Button<'a> {
    /// Creates a medium-size secondary button.
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            variant: ButtonVariant::Secondary,
            size: ButtonSize::Medium,
            enabled: true,
        }
    }

    /// Visual variant.
    #[must_use]
    pub fn variant(mut self, variant: ButtonVariant) -> Self {
        self.variant = variant;
        self
    }

    /// Button size.
    #[must_use]
    pub fn size(mut self, size: ButtonSize) -> Self {
        self.size = size;
        self
    }

    /// Enabled / disabled state.
    #[must_use]
    pub fn enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Displays the button and returns the egui response.
    ///
    /// Hover/pressed are handled by egui: the idle/hover tints
    /// ([`fills_for`], all from the theme) are injected into the local
    /// `visuals` instead of overwriting `fill` (which killed hover).
    pub fn show(self, ui: &mut egui::Ui, theme: &CygnusTheme) -> egui::Response {
        let text_color = match self.variant {
            ButtonVariant::Ghost => theme.colors.fg_secondary,
            ButtonVariant::Primary | ButtonVariant::Secondary | ButtonVariant::Danger => {
                theme.colors.fg_primary
            }
        };
        let height = match self.size {
            ButtonSize::Small => theme.sizes.button_sm,
            ButtonSize::Medium => theme.sizes.button_md,
            ButtonSize::Large => theme.sizes.button_lg,
        };
        let fills = fills_for(self.variant, theme);
        ui.scope(|ui| {
            let widgets = &mut ui.visuals_mut().widgets;
            widgets.inactive.bg_fill = fills.idle;
            widgets.hovered.bg_fill = fills.hover;
            widgets.active.bg_fill = fills.pressed;
            ui.add_enabled(
                self.enabled,
                egui::Button::new(
                    egui::RichText::new(self.label)
                        .size(theme.typography.body_size)
                        .color(text_color),
                )
                .corner_radius(theme.radius.sm)
                .min_size(egui::vec2(0.0, height)),
            )
        })
        .inner
    }
}

/// Menu row (bar or context popup): full-width ghost button;
/// `true` when clicked (the caller closes the menu).
pub fn menu_row(ui: &mut egui::Ui, theme: &CygnusTheme, label: &str) -> bool {
    Button::new(label)
        .variant(ButtonVariant::Ghost)
        .size(ButtonSize::Medium)
        .show(ui, theme)
        .clicked()
}

/// Idle/hover/pressed fills of a variant, 100% from the theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ButtonFills {
    idle: egui::Color32,
    hover: egui::Color32,
    pressed: egui::Color32,
}

/// Tints of a variant: hover always differs from idle for
/// visual feedback (even the ghost button, transparent at idle).
fn fills_for(variant: ButtonVariant, theme: &CygnusTheme) -> ButtonFills {
    let colors = theme.colors;
    match variant {
        ButtonVariant::Primary => ButtonFills {
            idle: colors.accent,
            hover: colors.accent_hover,
            pressed: colors.accent,
        },
        ButtonVariant::Secondary => ButtonFills {
            idle: colors.bg_tertiary,
            hover: colors.item_hover,
            pressed: colors.bg_tertiary,
        },
        ButtonVariant::Ghost => ButtonFills {
            idle: egui::Color32::TRANSPARENT,
            hover: colors.item_hover,
            pressed: colors.item_selected,
        },
        ButtonVariant::Danger => ButtonFills {
            idle: colors.error,
            hover: colors.error_hover,
            pressed: colors.error,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for variant in [
                    ButtonVariant::Primary,
                    ButtonVariant::Secondary,
                    ButtonVariant::Ghost,
                    ButtonVariant::Danger,
                ] {
                    for size in [ButtonSize::Small, ButtonSize::Medium, ButtonSize::Large] {
                        let _ = Button::new("Save")
                            .variant(variant)
                            .size(size)
                            .enabled(true)
                            .show(ui, &theme);
                    }
                }
                let _ = Button::new("Off").enabled(false).show(ui, &theme);
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn hover_differs_from_idle_for_every_variant() {
        let theme = CygnusTheme::dark();
        for variant in [
            ButtonVariant::Primary,
            ButtonVariant::Secondary,
            ButtonVariant::Ghost,
            ButtonVariant::Danger,
        ] {
            let fills = fills_for(variant, &theme);
            assert_ne!(
                fills.hover, fills.idle,
                "pas de retour au survol pour {variant:?}"
            );
        }
        // Ghost: invisible at idle, underlined on hover/press.
        let ghost = fills_for(ButtonVariant::Ghost, &theme);
        assert_eq!(ghost.idle, egui::Color32::TRANSPARENT);
        assert_eq!(ghost.hover, theme.colors.item_hover);
        assert_eq!(ghost.pressed, theme.colors.item_selected);
    }

    #[test]
    fn menu_row_renders_and_reports_no_click_idle() {
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

    #[test]
    fn button_reports_hovered_state() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        // Frame 1: layout, capture the button's rectangle.
        let mut rect = egui::Rect::NOTHING;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                rect = Button::new("Save").show(ui, &theme).rect;
            });
        })
        .drop_without_applying_deltas();
        assert!(!rect.is_negative(), "bouton mis en page");
        // Frame 2: pointer at the center → the hovered state comes up.
        let mut input = egui::RawInput::default();
        input.events.push(egui::Event::PointerMoved(rect.center()));
        let mut hovered = false;
        ctx.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                hovered = Button::new("Save").show(ui, &theme).hovered();
            });
        })
        .drop_without_applying_deltas();
        assert!(hovered, "le survol doit être détecté");
    }
}

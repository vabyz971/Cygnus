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

//! Standard Cygnus modal (title + content + action buttons).
//!
//! Open/close ownership sits with the app (`open: &mut bool`):
//! the modal only displays and reports the chosen action.
use crate::theme::UiThemeExt;
use crate::theme::typography::heading_text;

/// Action reported by [`CygnusModal::show`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModalAction {
    /// Primary button (confirm).
    Confirm,
    /// Secondary button (cancel) or close.
    Cancel,
}

/// Standard Cygnus modal.
///
/// # Example
/// ```rust
/// # use ui_kit::dialogs::CygnusModal;
/// # egui::__run_test_ui(|ui| {
/// let mut open = true;
/// CygnusModal::new("Exporter", "Valider", "Annuler").show(ui, &mut open, |ui| {
///     ui.label("options d'export");
/// });
/// # });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CygnusModal<'a> {
    title: &'a str,
    confirm_label: &'a str,
    cancel_label: &'a str,
    /// Base size (width × height) of the window.
    size: Option<egui::Vec2>,
}

impl<'a> CygnusModal<'a> {
    /// Creates a modal (title + button labels).
    pub fn new(title: &'a str, confirm_label: &'a str, cancel_label: &'a str) -> Self {
        Self {
            title,
            confirm_label,
            cancel_label,
            size: None,
        }
    }

    /// Sets the window's base size (clamped to the screen).
    #[must_use]
    pub fn size(mut self, size: impl Into<egui::Vec2>) -> Self {
        self.size = Some(size.into());
        self
    }

    /// Shows the modal when `open`. Returns the resulting action, if any
    /// (and closes on Confirm/Cancel).
    pub fn show<R>(
        self,
        ctx: &egui::Context,
        open: &mut bool,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> Option<ModalAction> {
        if !*open {
            return None;
        }
        let theme = ctx.cygnus_theme();
        let mut action = None;

        // The window is bounded to the screen so it never hides the
        // action buttons: max width/height = ~92% of the screen and
        // content scrolls when needed.
        let available = ctx.input(|i| i.raw.screen_rect);
        let available_w = available.map_or(theme.spacing.xl * 12.0, |r| {
            r.width() - theme.spacing.xl * 2.0
        });
        let available_h = available.map_or(theme.spacing.xl * 10.0, |r| {
            r.height() - theme.spacing.xl * 2.0
        });
        let requested = self.size.unwrap_or(egui::vec2(360.0, 240.0));
        let max_w = available_w.max(280.0);
        let max_h = available_h.max(240.0);

        egui::Window::new(heading_text(&theme, self.title))
            .collapsible(false)
            .resizable(true)
            .drag_area(egui::WindowDrag::TitleBar)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .default_width(requested.x.min(max_w))
            .default_height(requested.y.min(max_h))
            .max_width(max_w)
            .max_height(max_h)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .max_height((max_h - theme.spacing.lg * 2.0).max(160.0))
                    .show(ui, |ui| add_contents(ui));
                ui.separator();
                ui.horizontal(|ui| {
                    if ui.button(self.cancel_label).clicked() {
                        action = Some(ModalAction::Cancel);
                    }
                    if ui.button(self.confirm_label).clicked() {
                        action = Some(ModalAction::Confirm);
                    }
                });
            });
        if action.is_some() {
            *open = false;
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modal_closed_renders_nothing() {
        let ctx = egui::Context::default();
        let mut open = false;
        let mut shown = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let action = CygnusModal::new("Titre", "OK", "Non").show(ui.ctx(), &mut open, |_ui| {
                shown = true;
            });
            assert_eq!(action, None);
        })
        .drop_without_applying_deltas();
        assert!(!shown);
        assert!(!open);
    }

    #[test]
    fn modal_open_renders_without_panic() {
        let ctx = egui::Context::default();
        let mut open = true;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = CygnusModal::new("Exporter", "Valider", "Annuler").show(
                ui.ctx(),
                &mut open,
                |ui| {
                    ui.label("contenu");
                },
            );
        })
        .drop_without_applying_deltas();
        assert!(open, "sans clic, la modale reste ouverte");
    }
}

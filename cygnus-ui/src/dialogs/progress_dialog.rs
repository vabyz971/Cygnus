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

//! Progress dialog for long operations (export…).
//!
//! Work stays on a background thread; the app feeds `fraction`
//! from its channel and can cancel via `cancel_requested`.

use crate::theme::UiThemeExt;
use crate::theme::typography::heading_text;

/// Progress dialog (export, render…).
///
/// # Example
/// ```rust
/// # use cygnus_ui::dialogs::CygnusProgressDialog;
/// # egui::__run_test_ui(|ui| {
/// let mut open = true;
/// let mut cancel = false;
/// CygnusProgressDialog::new("Export en cours").show(
///     ui.ctx(), &mut open, 0.42, "image.png", &mut cancel,
/// );
/// # });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CygnusProgressDialog<'a> {
    title: &'a str,
}

impl<'a> CygnusProgressDialog<'a> {
    /// Creates a progress dialog.
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }

    /// Shows the dialog when `open`: 0..=1 bar, current task,
    /// Cancel button (sets `cancel_requested`).
    pub fn show(
        self,
        ctx: &egui::Context,
        open: &mut bool,
        fraction: f32,
        current_task: &str,
        cancel_requested: &mut bool,
    ) {
        if !*open {
            return;
        }
        let theme = ctx.cygnus_theme();
        egui::Window::new(heading_text(&theme, self.title))
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(current_task);
                ui.add(egui::ProgressBar::new(fraction.clamp(0.0, 1.0)).show_percentage());
                ui.horizontal(|ui| {
                    if ui.button("Annuler").clicked() {
                        *cancel_requested = true;
                        *open = false;
                    }
                });
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_renders_without_panic() {
        let ctx = egui::Context::default();
        let mut open = true;
        let mut cancel = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            CygnusProgressDialog::new("Export").show(
                ui.ctx(),
                &mut open,
                0.42,
                "image.png",
                &mut cancel,
            );
            // Out-of-bounds fraction clamped without panic.
            CygnusProgressDialog::new("Export").show(
                ui.ctx(),
                &mut open,
                9.9,
                "image.png",
                &mut cancel,
            );
        })
        .drop_without_applying_deltas();
        assert!(open);
        assert!(!cancel);
    }

    #[test]
    fn progress_closed_renders_nothing() {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let mut open = false;
            let mut cancel = false;
            CygnusProgressDialog::new("Export").show(ui.ctx(), &mut open, 0.5, "x", &mut cancel);
        });
        assert!(output.shapes.is_empty());
        output.drop_without_applying_deltas();
    }
}

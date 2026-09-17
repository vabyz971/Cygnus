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

//! Section repliable standard Cygnus.
//!
//! En-tête cliquable (chevron peint, sans glyphe) + contenu indenté.
//! L'état ouvert/fermé est détenu par l'app via `&mut bool`, ce qui
//! rend la bascule testable sans contexte UI.

use crate::theme::CygnusTheme;
use crate::theme::typography::heading_text;

/// Section repliable standard Cygnus.
///
/// # Exemple
/// ```rust
/// # use ui_kit::panels::CygnusCollapsible;
/// # egui::__run_test_ui(|ui| {
/// let mut open = true;
/// CygnusCollapsible::new("Avance").show(ui, &mut open, |ui| {
///     ui.label("options avancees");
/// });
/// # });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct CygnusCollapsible<'a> {
    title: &'a str,
}

impl<'a> CygnusCollapsible<'a> {
    /// Crée une section repliable avec le titre donné.
    pub fn new(title: &'a str) -> Self {
        Self { title }
    }

    /// Dessine le chevron d'ouverture (peint, sans glyphe).
    /// Chevron ">" fermé, "v" ouvert.
    fn paint_chevron(ui: &mut egui::Ui, rect: egui::Rect, open: bool) {
        let theme = CygnusTheme::dark();
        let stroke = egui::Stroke::new(1.5, theme.colors.fg_secondary);
        let center = rect.center();
        let half = 4.0;
        if open {
            ui.painter().line_segment(
                [
                    egui::pos2(center.x - half, center.y - half),
                    egui::pos2(center.x, center.y + half),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    egui::pos2(center.x, center.y + half),
                    egui::pos2(center.x + half, center.y - half),
                ],
                stroke,
            );
        } else {
            ui.painter().line_segment(
                [
                    egui::pos2(center.x - half, center.y - half),
                    egui::pos2(center.x + half, center.y),
                ],
                stroke,
            );
            ui.painter().line_segment(
                [
                    egui::pos2(center.x + half, center.y),
                    egui::pos2(center.x - half, center.y + half),
                ],
                stroke,
            );
        }
    }

    /// Affiche l'en-tête puis, si ouvert, le contenu indenté.
    /// Retourne la réponse de l'en-tête et, si ouvert, celle du corps.
    pub fn show<R>(
        self,
        ui: &mut egui::Ui,
        open: &mut bool,
        add_contents: impl FnOnce(&mut egui::Ui) -> R,
    ) -> (egui::Response, Option<egui::InnerResponse<R>>) {
        let theme = CygnusTheme::dark();
        let header = ui.horizontal(|ui| {
            let (rect, chevron) =
                ui.allocate_exact_size(egui::vec2(16.0, 16.0), egui::Sense::click());
            Self::paint_chevron(ui, rect, *open);
            let label = ui.selectable_label(*open, heading_text(&theme, self.title));
            if chevron.clicked() || label.clicked() {
                *open = !*open;
            }
        });
        let body = if *open {
            Some(ui.indent(self.title, add_contents))
        } else {
            None
        };
        (header.response, body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsible_toggle() {
        let ctx = egui::Context::default();

        // Fermé : le corps n'est pas affiché.
        let mut open = false;
        let mut body_shown = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            CygnusCollapsible::new("Section").show(ui, &mut open, |_ui| {
                body_shown = true;
            });
        })
        .drop_without_applying_deltas();
        assert!(!open);
        assert!(!body_shown);

        // Ouvert : le corps est affiché.
        open = true;
        body_shown = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            CygnusCollapsible::new("Section").show(ui, &mut open, |_ui| {
                body_shown = true;
            });
        })
        .drop_without_applying_deltas();
        assert!(open);
        assert!(body_shown);
    }
}

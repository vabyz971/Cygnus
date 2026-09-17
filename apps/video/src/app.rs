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

//! App Video minimale : placeholder en attendant `video-engine`.

use ui_kit::theme::CygnusTheme;
use ui_kit::theme::typography::{body_text, heading_text};

/// App Video (base minimale).
#[derive(Default)]
pub struct VideoApp {
    _private: (),
}

impl VideoApp {
    /// Crée l'app.
    pub fn new() -> Self {
        Self::default()
    }

    /// Dessine le placeholder central.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        let theme = CygnusTheme::dark();
        egui::CentralPanel::default().show(ui, |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(heading_text(&theme, "Cygnus Video"));
                ui.label(body_text(
                    &theme,
                    "Moteur en fondation : l'interface arrivera avec video-engine.",
                ));
            });
        });
    }
}

impl eframe::App for VideoApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_draws_without_panic() {
        let mut app = VideoApp::new();
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            app.draw(ui);
        })
        .drop_without_applying_deltas();
    }
}

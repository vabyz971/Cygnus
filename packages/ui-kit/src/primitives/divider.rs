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

//! Séparateur horizontal calibré sur le thème.

use crate::theme::CygnusTheme;

/// Séparateur horizontal avec les espacements du thème.
pub fn divider(ui: &mut egui::Ui, theme: &CygnusTheme) {
    ui.add_space(theme.spacing.xs);
    ui.separator();
    ui.add_space(theme.spacing.xs);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn divider_renders_without_panic() {
        let theme = CygnusTheme::dark();
        let ctx = egui::Context::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                divider(ui, &theme);
            });
        })
        .drop_without_applying_deltas();
    }
}

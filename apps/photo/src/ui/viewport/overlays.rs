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

//! Overlays photo : marqueurs par-dessus le viewport générique.
//!
//! Couche de composition : la géométrie vient du [`ViewportState`](ui_kit::viewport::ViewportState),
//! le dessin des helpers agnostiques
//! ([`draw_crosshair`](ui_kit::viewport::draw_crosshair)), le style du
//! thème.

use crate::commands::PhotoUiContext;
use ui_kit::viewport::{ViewportState, draw_crosshair};

/// Croix à l'origine monde (0, 0) du viewport.
pub fn draw_origin_marker(painter: &egui::Painter, ctx: &PhotoUiContext, viewport: &ViewportState) {
    let theme = ctx.shared.theme();
    draw_crosshair(
        painter,
        viewport.world_to_screen(egui::Vec2::ZERO),
        theme.spacing.sm,
        egui::Stroke::new(theme.borders.thin, theme.colors.fg_secondary),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlays_render_without_panic() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        let viewport = ViewportState::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                draw_origin_marker(ui.painter(), &photo_ctx, &viewport);
            });
        })
        .drop_without_applying_deltas();
    }
}

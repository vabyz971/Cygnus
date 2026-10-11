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

//! Minimal base of the Audio app (prompt v2): eframe boot + theme.
//!
//! `audio-engine` is still foundation-only, so no complex UI for
//! now: a central placeholder. Transport, tracks,
//! piano roll and mixer will be built here when the engine
//! arrives.

mod app;

fn main() {
    eframe::run_native(
        "Cygnus Audio",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            ui_kit::theme::setup_fonts(&cc.egui_ctx);
            ui_kit::theme::apply_cygnus_theme(&cc.egui_ctx);
            Ok(Box::new(app::AudioApp::new()))
        }),
    )
    .expect("Failed to start eframe");
}

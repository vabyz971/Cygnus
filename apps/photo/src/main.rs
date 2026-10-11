// Cygnus — Professional open-source creative suite
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

//! Photo app entry point: eframe boot + Cygnus theme.
//!
//! Breakdown:
//! - `app`        : PhotoApp (orchestrator: state + commands + engine)
//! - `state`      : state layers (document, shell, runtime)
//! - `commands`   : PhotoAction + queue + UI context
//! - `layout`     : workspace (top/left/center/right/bottom regions)
//! - `ui/`        : domain features, viewport, flat widgets
//! - `persistence`: workspace save (independent of the document)

mod app;
mod commands;
mod i18n;
mod layout;
mod persistence;
mod shortcuts;
mod state;
mod ui;

fn main() {
    eframe::run_native(
        "Cygnus Photo",
        eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 900.0]),
            ..Default::default()
        },
        Box::new(|cc| {
            ui_kit::theme::setup_fonts(&cc.egui_ctx);
            ui_kit::theme::apply_cygnus_theme(&cc.egui_ctx);
            Ok(Box::new(app::PhotoApp::new()))
        }),
    )
    .expect("Failed to start eframe");
}

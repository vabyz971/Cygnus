// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Cygnus Photo application binary: assembles the user interface, the
//! `photo` crates and the MCP server, and holds the Photo-specific panels
//! (Layers, Adjustments).

mod app;
mod i18n;
mod keys;
mod panels;
mod placeholder;

use std::process::ExitCode;

use cygnus_ui_kit::i18n::tr;
use cygnus_ui_kit::theme;

fn main() -> ExitCode {
    i18n::install();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr(keys::APP_NAME))
            .with_app_id("cygnus-photo")
            .with_inner_size([1600.0, 1000.0])
            .with_min_inner_size([1024.0, 640.0]),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    let result = eframe::run_native(
        "cygnus-photo",
        options,
        Box::new(|creation| {
            theme::install(&creation.egui_ctx);
            Ok(Box::new(app::PhotoApp::new()))
        }),
    );

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cygnus-photo: {error}");
            ExitCode::FAILURE
        }
    }
}

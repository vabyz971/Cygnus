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
mod bench;
mod canvas;
mod demo;
mod i18n;
mod keys;
mod panels;
mod placeholder;

use std::process::ExitCode;
use std::sync::Arc;

use cygnus_photo_render::gpu;
use eframe::egui_wgpu;

use cygnus_ui_kit::i18n::tr;
use cygnus_ui_kit::theme;

/// GPU setup of the window: the same adapter rules and device requirements
/// as the headless renderer (docs/decisions/0012 and 0014). An adapter that
/// does not meet them is refused at start-up with the list of what it
/// lacks.
fn wgpu_options(no_vsync: bool) -> egui_wgpu::WgpuConfiguration {
    let mut setup = egui_wgpu::WgpuSetupCreateNew::without_display_handle();
    setup.instance_descriptor.backends = gpu::BACKENDS;
    setup.device_descriptor = Arc::new(gpu::device_descriptor);
    setup.native_adapter_selector = Some(Arc::new(|adapters, surface| {
        gpu::choose_adapter(adapters, |adapter| {
            surface.is_none_or(|surface| adapter.is_surface_supported(surface))
        })
        .map_err(|error| error.to_string())
    }));
    let mut configuration = egui_wgpu::WgpuConfiguration {
        wgpu_setup: egui_wgpu::WgpuSetup::CreateNew(setup),
        ..Default::default()
    };
    if no_vsync {
        // The benchmark measures frames, not the display refresh rate.
        configuration.surface.present_mode = eframe::wgpu::PresentMode::AutoNoVsync;
    }
    configuration
}

fn main() -> ExitCode {
    i18n::install();

    // Development mode: fluidity benchmark (docs/decisions/0014).
    let bench = match std::env::var(bench::ENV).ok().as_deref() {
        None => false,
        Some("fluidity") => true,
        Some(other) => {
            eprintln!("cygnus-photo: unknown benchmark {other:?} (expected \"fluidity\")");
            return ExitCode::from(2);
        }
    };

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(tr(keys::APP_NAME))
            .with_app_id("cygnus-photo")
            .with_inner_size(if bench {
                [2560.0, 1440.0]
            } else {
                [1600.0, 1000.0]
            })
            .with_min_inner_size([1024.0, 640.0]),
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: wgpu_options(bench),
        ..Default::default()
    };

    let result = eframe::run_native(
        "cygnus-photo",
        options,
        Box::new(|creation| {
            theme::install(&creation.egui_ctx);
            let render_state = creation
                .wgpu_render_state
                .as_ref()
                .ok_or("the wgpu renderer is not available")?;
            canvas::install(render_state)?;
            if bench {
                Ok(Box::new(app::PhotoApp::new_bench()?))
            } else {
                Ok(Box::new(app::PhotoApp::new()))
            }
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

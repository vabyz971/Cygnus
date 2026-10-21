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
mod layout;
mod persistence;
mod shortcuts;
mod state;
mod ui;

fn main() -> std::process::ExitCode {
    let started = eframe::run_native(
        "Cygnus Photo",
        eframe::NativeOptions {
            // Client-side chrome (`ui::window_chrome`): the brand,
            // menus and window buttons share the first row.
            viewport: egui::ViewportBuilder::default()
                .with_title("Cygnus Photo")
                .with_inner_size([1566.0, 979.0])
                .with_min_inner_size([960.0, 600.0])
                .with_decorations(false),
            ..Default::default()
        },
        Box::new(|cc| {
            cygnus_ui::theme::setup_fonts(&cc.egui_ctx);
            cygnus_ui::theme::apply_cygnus_theme(&cc.egui_ctx);
            Ok(Box::new(
                app::PhotoApp::new()
                    .with_repaint_context(cc.egui_ctx.clone())
                    .with_render_state(cc.wgpu_render_state.clone()),
            ))
        }),
    );
    match started {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            // The debug form names the cause (`NoWaylandLib`…), the display
            // form reads better: both feed the hints.
            let cause = format!("{error} ({error:?})");
            eprintln!("{}", startup_failure_message(&cause));
            std::process::ExitCode::FAILURE
        }
    }
}

/// Readable startup failure (no panic): the cause, plus the usual fix
/// when the display or GPU libraries could not be loaded.
fn startup_failure_message(error: &str) -> String {
    let mut message = format!("Cygnus Photo could not start: {error}");
    let missing_libs = [
        "NoWaylandLib",
        "NoX11",
        "XNotSupported",
        "LibraryOpenError",
        "library could not be loaded",
    ]
    .iter()
    .any(|marker| error.contains(marker));
    if missing_libs {
        message.push_str(
            "\n\nThe display libraries (Wayland or X11, xkbcommon, Vulkan) were not found.\n\
             - Debian / Ubuntu: sudo apt install libwayland-client0 libxkbcommon0 libvulkan1\n\
             - Fedora: sudo dnf install libwayland-client libxkbcommon vulkan-loader\n\
             - Arch: sudo pacman -S wayland libxkbcommon vulkan-icd-loader\n\
             - NixOS: run `nix run github:vabyz971/Cygnus` (the downloaded binary\n\
               cannot find libraries in /nix/store on its own).",
        );
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_display_libraries_get_install_hints() {
        let message = startup_failure_message("WaylandError(Connection(NoWaylandLib))");
        assert!(message.contains("NoWaylandLib"), "cause kept");
        assert!(message.contains("nix run"), "NixOS hint");
        assert!(message.contains("apt install"), "distribution hints");
        let other = startup_failure_message("no adapter");
        assert!(
            !other.contains("nix run"),
            "hints only for missing libraries"
        );
    }
}

//! Gallery — visual examples of the ui-kit components.
//
//! Displays the ui-kit widgets (buttons, sliders, tables, containers)
//! with the default dark theme.
//
//! Run with:
//! ```bash
//! cargo run --package ui-kit --example gallery
//! ```
//!
//! This example shows:
//! - The theme tokens (`ui_kit::theme::CygnusTheme`)
//! - The base components via the `prelude`
//! - Integration with egui and eframe

use eframe::egui;

use ui_kit::prelude::*;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([800.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Gallery — ui-kit",
        options,
        Box::new(|_cc| {
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(
                Box::new(GalleryApp {}) as Box<dyn eframe::App>
            )
        }),
    )
}

/// Example app: one page with several ui-kit widgets.
struct GalleryApp;

impl eframe::App for GalleryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Install the Cygnus theme
        install_theme(ui.ctx(), CygnusTheme::default());

        ui.heading("ui-kit Gallery");

        // Slider
        ui.add(egui::Slider::new(&mut 0.0, 0.0..=1.0).text("Opacité"));

        // Tabs
        ui.horizontal(|ui| {
            if ui.selectable_label(true, "Accueil").clicked() {
                // nothing yet
            }
            if ui.selectable_label(true, "Paramètres").clicked() {
                // nothing yet
            }
        });

        // Card container
        ui.group(|ui| {
            ui.label("C'est un groupe/conteneur");
        });

        // Plain area with text
        ui.label("Aperçu canvas (widget métier)");
    }
}

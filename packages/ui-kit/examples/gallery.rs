//! Gallery — d\'exemples visuels des composants ui-kit.
//
//! Affiche les widgets ui-kit (boutons, curseurs, tableaux, conteneurs)
//! avec le thème sombre par défaut.
//
//! Lancer avec :
//! ```bash
//! cargo run --package ui-kit --example gallery
//! ```
//!
//! Cet exemple montre :
//! - Les tokens du thème (`ui_kit::theme::CygnusTheme`)
//! - Les composants de base via la `prelude`
//! - L'intégration avec egui et eframe

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

/// Application d'exemple : une page avec plusieurs widgets ui-kit.
struct GalleryApp;

impl eframe::App for GalleryApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Installer le thème Cygnus
        install_theme(ui.ctx(), CygnusTheme::default());

        ui.heading("ui-kit Gallery");

        // Curseur
        ui.add(egui::Slider::new(&mut 0.0, 0.0..=1.0).text("Opacité"));

        // Onglets
        ui.horizontal(|ui| {
            if ui.selectable_label(true, "Accueil").clicked() {
                // nothing yet
            }
            if ui.selectable_label(true, "Paramètres").clicked() {
                // nothing yet
            }
        });

        // Conteneur card
        ui.group(|ui| {
            ui.label("C'est un groupe/conteneur");
        });

        // Zone simple avec texte
        ui.label("Aperçu canvas (widget métier)");
    }
}

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

//! Fenêtres de paramètres de l'app PHOTO (widgets métier).
//!
//! - Nouveau document : format prédéfini, dimensions dans l'unité
//!   choisie (px, in, cm, pica à [`DOC_DPI`] DPI), orientation
//!   portrait/paysage. Valider retourne les dimensions en pixels.
//! - Exportation : dossier, nom de fichier, format (PNG, JPEG, JPG,
//!   GIF), qualité JPEG, bouton de validation.
//! - Aide : rappels des outils et raccourcis.
//!
//! État détenu par l'app, dessin headless-testable, style
//! exclusivement ui-kit.

use std::path::PathBuf;
use ui_kit::components::{Button, ButtonVariant, NumberInput, Select, Slider, TextInput};
use ui_kit::dialogs::{CygnusModal, ModalAction};
use ui_kit::theme::CygnusTheme;
use ui_kit::theme::typography::body_text;

/// Résolution d'impression pour les unités physiques (in, cm, pica).
pub const DOC_DPI: f64 = 300.0;
/// Dimension maximale d'un document (garde-fou mémoire).
pub const DOC_MAX_DIMENSION: f64 = 16_000.0;

/// Formats prédéfinis de nouveau document (index 0 = personnalisé).
pub const DOC_PRESETS: &[&str] = &[
    "Personnalise",
    "Full HD 1920 x 1080",
    "4K 3840 x 2160",
    "Carre 1080 x 1080",
    "A4 portrait 300 DPI",
    "A3 paysage 300 DPI",
];

/// Unités de dimension (pica = 1/6 de pouce, unité typographique).
pub const DOC_UNITS: &[&str] = &["px", "in", "cm", "pica"];

/// Formats d'export proposés.
pub const EXPORT_FORMATS: &[&str] = &["PNG", "JPEG", "JPG", "GIF"];

/// Orientation d'un nouveau document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocOrientation {
    /// Hauteur >= largeur.
    #[default]
    Portrait,
    /// Largeur >= hauteur.
    Paysage,
}

/// Nombre de pixels par unité (unités physiques à [`DOC_DPI`] DPI).
#[must_use]
pub fn px_per_unit(unit: usize) -> f64 {
    match DOC_UNITS.get(unit).copied().unwrap_or("px") {
        "in" => DOC_DPI,
        "cm" => DOC_DPI / 2.54,
        "pica" => DOC_DPI / 6.0,
        _ => 1.0,
    }
}

/// Convertit une valeur saisie dans `unit` vers des pixels (bornés).
#[must_use]
pub fn to_px(value: f64, unit: usize) -> u32 {
    (value * px_per_unit(unit))
        .round()
        .clamp(1.0, DOC_MAX_DIMENSION) as u32
}

/// Convertit des pixels vers l'unité d'affichage.
#[must_use]
pub fn from_px(px: f64, unit: usize) -> f64 {
    px / px_per_unit(unit).max(1.0)
}

/// État de la fenêtre « Nouveau document ».
#[derive(Debug, Clone)]
pub struct NewDocumentDialogState {
    /// Fenêtre visible.
    pub open: bool,
    /// Index dans [`DOC_PRESETS`].
    pub preset: usize,
    /// Index dans [`DOC_UNITS`].
    pub unit: usize,
    /// Largeur dans l'unité courante.
    pub width: f64,
    /// Hauteur dans l'unité courante.
    pub height: f64,
    /// Orientation (bascule = échange si besoin).
    pub orientation: DocOrientation,
}

impl Default for NewDocumentDialogState {
    fn default() -> Self {
        Self {
            open: false,
            preset: 1,
            unit: 0,
            width: 1920.0,
            height: 1080.0,
            orientation: DocOrientation::Paysage,
        }
    }
}

impl NewDocumentDialogState {
    /// Ouvre la fenêtre sur le format Full HD.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Dimensions finales en pixels (bornées, au moins 1x1).
    #[must_use]
    pub fn px_dimensions(&self) -> (u32, u32) {
        (to_px(self.width, self.unit), to_px(self.height, self.unit))
    }

    /// Applique un format prédéfini (retour en px, orientation déduite).
    pub fn apply_preset(&mut self, preset: usize) {
        self.preset = preset;
        let (width, height) = match preset {
            1 => (1920.0, 1080.0),
            2 => (3840.0, 2160.0),
            3 => (1080.0, 1080.0),
            4 => (from_px(2480.0, self.unit), from_px(3508.0, self.unit)),
            5 => (from_px(4961.0, self.unit), from_px(3508.0, self.unit)),
            _ => return,
        };
        self.unit = 0;
        self.width = width;
        self.height = height;
        self.orientation = if width >= height {
            DocOrientation::Paysage
        } else {
            DocOrientation::Portrait
        };
    }

    /// Change d'unité en conservant les dimensions pixels.
    pub fn set_unit(&mut self, unit: usize) {
        let (px_w, px_h) = self.px_dimensions();
        self.unit = unit;
        self.width = from_px(f64::from(px_w), unit);
        self.height = from_px(f64::from(px_h), unit);
        self.preset = 0;
    }

    /// Bascule l'orientation (échange les dimensions si besoin).
    pub fn set_orientation(&mut self, orientation: DocOrientation) {
        self.orientation = orientation;
        match orientation {
            DocOrientation::Portrait => {
                if self.width > self.height {
                    std::mem::swap(&mut self.width, &mut self.height);
                }
            }
            DocOrientation::Paysage => {
                if self.height > self.width {
                    std::mem::swap(&mut self.width, &mut self.height);
                }
            }
        }
        self.preset = 0;
    }
}

/// Requête d'export validée (dossier + nom + format + qualité).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRequest {
    /// Chemin complet de destination (extension = format).
    pub path: PathBuf,
    /// Qualité JPEG 1..=100.
    pub quality: u8,
}

/// État de la fenêtre « Exportation ».
#[derive(Debug, Clone)]
pub struct ExportDialogState {
    /// Fenêtre visible.
    pub open: bool,
    /// Dossier de destination (champ texte).
    pub folder: String,
    /// Nom de fichier sans extension.
    pub filename: String,
    /// Index dans [`EXPORT_FORMATS`].
    pub format: usize,
    /// Qualité JPEG 1..=100.
    pub quality: f32,
}

impl Default for ExportDialogState {
    fn default() -> Self {
        Self {
            open: false,
            folder: std::env::temp_dir().display().to_string(),
            filename: String::from("export"),
            format: 0,
            quality: 90.0,
        }
    }
}

impl ExportDialogState {
    /// Ouvre la fenêtre d'export.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Construit la requête validée (nom assaini, extension = format).
    #[must_use]
    pub fn request(&self) -> ExportRequest {
        let ext = EXPORT_FORMATS
            .get(self.format)
            .copied()
            .unwrap_or("PNG")
            .to_ascii_lowercase();
        let stem = self.filename.trim();
        let stem = if stem.is_empty() { "export" } else { stem };
        ExportRequest {
            path: PathBuf::from(self.folder.clone()).join(format!("{stem}.{ext}")),
            quality: self.quality.round().clamp(1.0, 100.0) as u8,
        }
    }
}

/// Dessine la fenêtre « Nouveau document ». Retourne les dimensions
/// pixels à la validation (`None` sinon ou si la fenêtre est fermée).
pub fn draw_new_document_dialog(
    ctx: &egui::Context,
    state: &mut NewDocumentDialogState,
) -> Option<(u32, u32)> {
    if !state.open {
        return None;
    }
    let theme = CygnusTheme::dark();
    let mut confirm: Option<(u32, u32)> = None;
    let mut open = true;
    let action =
        CygnusModal::new("Nouveau document", "Creer", "Annuler").show(ctx, &mut open, |ui| {
            let presets: Vec<&str> = DOC_PRESETS.to_vec();
            let mut preset = state.preset;
            Select::new("Format", &presets).show(ui, &theme, &mut preset);
            if preset != state.preset {
                state.apply_preset(preset);
            }
            let units: Vec<&str> = DOC_UNITS.to_vec();
            let mut unit = state.unit;
            Select::new("Unite", &units).show(ui, &theme, &mut unit);
            if unit != state.unit {
                state.set_unit(unit);
            }
            let max = from_px(DOC_MAX_DIMENSION, state.unit);
            let mut width = state.width;
            let mut height = state.height;
            NumberInput::new("Largeur")
                .range(0.01..=max)
                .show(ui, &theme, &mut width);
            NumberInput::new("Hauteur")
                .range(0.01..=max)
                .show(ui, &theme, &mut height);
            if width != state.width || height != state.height {
                state.width = width;
                state.height = height;
                state.preset = 0;
            }
            ui.horizontal(|ui| {
                ui.label(body_text(&theme, "Orientation"));
                let portrait = state.orientation == DocOrientation::Portrait;
                if Button::new("Portrait")
                    .variant(if portrait {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    })
                    .show(ui, &theme)
                    .clicked()
                {
                    state.set_orientation(DocOrientation::Portrait);
                }
                let paysage = state.orientation == DocOrientation::Paysage;
                if Button::new("Paysage")
                    .variant(if paysage {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    })
                    .show(ui, &theme)
                    .clicked()
                {
                    state.set_orientation(DocOrientation::Paysage);
                }
            });
            let (px_w, px_h) = state.px_dimensions();
            ui.label(body_text(&theme, &format!("{px_w} x {px_h} px")));
        });
    state.open = open;
    if action == Some(ModalAction::Confirm) {
        confirm = Some(state.px_dimensions());
    }
    confirm
}

/// Dessine la fenêtre « Exportation ». Retourne la requête validée
/// (bouton de validation de la modale).
pub fn draw_export_dialog(
    ctx: &egui::Context,
    state: &mut ExportDialogState,
) -> Option<ExportRequest> {
    if !state.open {
        return None;
    }
    let theme = CygnusTheme::dark();
    let mut confirm: Option<ExportRequest> = None;
    let mut open = true;
    let action = CygnusModal::new("Exportation", "Valider", "Annuler").show(ctx, &mut open, |ui| {
        ui.horizontal(|ui| {
            ui.label(body_text(&theme, "Dossier"));
            TextInput::new().placeholder("Dossier d'exportation").show(
                ui,
                &theme,
                &mut state.folder,
            );
        });
        ui.horizontal(|ui| {
            ui.label(body_text(&theme, "Nom"));
            TextInput::new()
                .placeholder("Nom du fichier")
                .show(ui, &theme, &mut state.filename);
        });
        let formats: Vec<&str> = EXPORT_FORMATS.to_vec();
        Select::new("Format", &formats).show(ui, &theme, &mut state.format);
        if EXPORT_FORMATS.get(state.format).copied().unwrap_or("PNG") != "PNG"
            && EXPORT_FORMATS.get(state.format).copied().unwrap_or("PNG") != "GIF"
        {
            Slider::new("Qualite", 1.0..=100.0).show(ui, &theme, &mut state.quality);
        }
        let request = state.request();
        ui.label(body_text(
            &theme,
            &format!("Destination : {}", request.path.display()),
        ));
    });
    state.open = open;
    if action == Some(ModalAction::Confirm) {
        confirm = Some(state.request());
    }
    confirm
}

/// Dessine la fenêtre d'aide (outils, calques, raccourcis).
pub fn draw_help_dialog(ctx: &egui::Context, open: &mut bool) {
    if !*open {
        return;
    }
    let theme = CygnusTheme::dark();
    let mut stays_open = true;
    CygnusModal::new("Aide de Photo", "Fermer", "Fermer").show(ctx, &mut stays_open, |ui| {
        ui.label(body_text(
            &theme,
            "Outils : deplacement, main, loupe, pinceau, gomme, pipette.",
        ));
        ui.label(body_text(
            &theme,
            "Calques : clic = selection, double-clic sur le nom = renommer,",
        ));
        ui.label(body_text(
            &theme,
            "glisser-deposer = reordonner. Bas du panneau : image, vide,",
        ));
        ui.label(body_text(&theme, "dupliquer, masque, filtres, supprimer."));
        ui.label(body_text(
            &theme,
            "Fichier > Nouveau document : format, dimensions (px, in, cm,",
        ));
        ui.label(body_text(
            &theme,
            "pica), orientation portrait/paysage. Exportation : dossier,",
        ));
        ui.label(body_text(&theme, "format PNG/JPEG/JPG/GIF, validation."));
    });
    *open = stays_open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn units_convert_at_doc_dpi() {
        assert_eq!(px_per_unit(0), 1.0);
        assert_eq!(px_per_unit(1), DOC_DPI);
        assert!((px_per_unit(2) - DOC_DPI / 2.54).abs() < 1e-9);
        assert!((px_per_unit(3) - DOC_DPI / 6.0).abs() < 1e-9);
        assert_eq!(to_px(1920.0, 0), 1920);
        assert_eq!(to_px(1.0, 1), 300);
        assert_eq!(to_px(0.0, 0), 1);
        assert_eq!(to_px(1e9, 0), DOC_MAX_DIMENSION as u32);
        assert!((from_px(300.0, 1) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn preset_applies_px_and_orientation() {
        let mut state = NewDocumentDialogState::default();
        state.apply_preset(3);
        assert_eq!((state.width, state.height), (1080.0, 1080.0));
        state.set_orientation(DocOrientation::Portrait);
        state.width = 200.0;
        state.height = 100.0;
        state.set_orientation(DocOrientation::Portrait);
        assert_eq!((state.width, state.height), (100.0, 200.0));
        state.set_orientation(DocOrientation::Paysage);
        assert_eq!((state.width, state.height), (200.0, 100.0));
    }

    #[test]
    fn unit_switch_keeps_pixel_size() {
        let mut state = NewDocumentDialogState {
            width: 600.0,
            height: 300.0,
            ..Default::default()
        };
        state.set_unit(1);
        assert_eq!(state.px_dimensions(), (600, 300));
        assert!((state.width - 2.0).abs() < 1e-9);
    }

    #[test]
    fn export_request_builds_path_and_quality() {
        let mut state = ExportDialogState {
            folder: String::from("/tmp/cygnus"),
            filename: String::from("rendu"),
            format: 1,
            quality: 80.4,
            ..Default::default()
        };
        let request = state.request();
        assert_eq!(request.path, PathBuf::from("/tmp/cygnus/rendu.jpeg"));
        assert_eq!(request.quality, 80);
        state.filename = String::from("   ");
        assert_eq!(
            state.request().path,
            PathBuf::from("/tmp/cygnus/export.jpeg")
        );
    }

    #[test]
    fn dialogs_render_without_panic() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let mut new_doc = NewDocumentDialogState {
            open: true,
            ..Default::default()
        };
        let mut export = ExportDialogState {
            open: true,
            ..Default::default()
        };
        let mut help = true;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            let _ = draw_new_document_dialog(&ctx, &mut new_doc);
            let _ = draw_export_dialog(&ctx, &mut export);
            draw_help_dialog(&ctx, &mut help);
        })
        .drop_without_applying_deltas();
        assert!(new_doc.open && export.open && help);
    }

    #[test]
    fn dialogs_closed_do_nothing() {
        let ctx = egui::Context::default();
        let mut new_doc = NewDocumentDialogState::default();
        let mut export = ExportDialogState::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            assert_eq!(draw_new_document_dialog(&ctx, &mut new_doc), None);
            assert_eq!(draw_export_dialog(&ctx, &mut export), None);
        })
        .drop_without_applying_deltas();
    }
}

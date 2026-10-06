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
// MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Dialogues export et aide de l'app PHOTO.

use std::path::PathBuf;
use ui_kit::components::{Select, Slider, TextInput};
use ui_kit::dialogs::{CygnusModal, ModalAction};
use ui_kit::theme::typography::body_text;

/// Formats d'export proposés.
pub const EXPORT_FORMATS: &[&str] = &["PNG", "JPEG", "JPG", "GIF"];

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

/// Dessine la fenêtre « Exportation ». Retourne la requête validée
/// (bouton de validation de la modale).
pub fn draw_export_dialog(
    ui_ctx: &crate::commands::PhotoUiContext,
    state: &mut ExportDialogState,
) -> Option<ExportRequest> {
    if !state.open {
        return None;
    }
    let ctx = ui_ctx.shared.ctx();
    let theme = *ui_ctx.shared.theme();
    let translator = ui_ctx.shared.translator();
    let texts = crate::i18n::PhotoCatalog::new(translator.language());
    let mut confirm: Option<ExportRequest> = None;
    let mut open = true;
    let action = CygnusModal::new(
        texts.get(crate::i18n::PhotoTextKey::ExportTitle),
        translator.get(ui_kit::i18n::TextKey::Validate),
        translator.get(ui_kit::i18n::TextKey::Cancel),
    )
    .show(ctx, &mut open, |ui| {
        ui.horizontal(|ui| {
            ui.label(body_text(
                &theme,
                texts.get(crate::i18n::PhotoTextKey::FolderLabel),
            ));
            TextInput::new()
                .placeholder(texts.get(crate::i18n::PhotoTextKey::FolderPlaceholder))
                .show(ui, &theme, &mut state.folder);
        });
        ui.horizontal(|ui| {
            ui.label(body_text(
                &theme,
                texts.get(crate::i18n::PhotoTextKey::NameLabel),
            ));
            TextInput::new()
                .placeholder(texts.get(crate::i18n::PhotoTextKey::FileNamePlaceholder))
                .show(ui, &theme, &mut state.filename);
        });
        let formats: Vec<&str> = EXPORT_FORMATS.to_vec();
        Select::new(texts.get(crate::i18n::PhotoTextKey::FormatLabel), &formats).show(
            ui,
            &theme,
            &mut state.format,
        );
        if EXPORT_FORMATS.get(state.format).copied().unwrap_or("PNG") != "PNG"
            && EXPORT_FORMATS.get(state.format).copied().unwrap_or("PNG") != "GIF"
        {
            Slider::new(
                texts.get(crate::i18n::PhotoTextKey::QualityLabel),
                1.0..=100.0,
            )
            .show(ui, &theme, &mut state.quality);
        }
        let request = state.request();
        ui.label(body_text(
            &theme,
            &format!(
                "{} {}",
                texts.get(crate::i18n::PhotoTextKey::DestinationLabel),
                request.path.display()
            ),
        ));
    });
    state.open = open;
    if action == Some(ModalAction::Confirm) {
        confirm = Some(state.request());
    }
    confirm
}

/// Dessine la fenêtre d'aide (outils, calques, raccourcis).
pub fn draw_help_dialog(ui_ctx: &crate::commands::PhotoUiContext, open: &mut bool) {
    if !*open {
        return;
    }
    let ctx = ui_ctx.shared.ctx();
    let theme = *ui_ctx.shared.theme();
    let translator = ui_ctx.shared.translator();
    let texts = crate::i18n::PhotoCatalog::new(translator.language());
    let mut stays_open = true;
    CygnusModal::new(
        texts.get(crate::i18n::PhotoTextKey::HelpTitle),
        translator.get(ui_kit::i18n::TextKey::Close),
        translator.get(ui_kit::i18n::TextKey::Close),
    )
    .show(ctx, &mut stays_open, |ui| {
        ui.label(body_text(
            &theme,
            texts.get(crate::i18n::PhotoTextKey::HelpToolsText),
        ));
        ui.label(body_text(
            &theme,
            texts.get(crate::i18n::PhotoTextKey::HelpLayersText),
        ));
        ui.label(body_text(
            &theme,
            texts.get(crate::i18n::PhotoTextKey::HelpFileText),
        ));
    });
    *open = stays_open;
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

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
// MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! PHOTO app export and help dialogs.

use cygnus_ui::components::{Select, Slider, TextInput};
use cygnus_ui::dialogs::{CygnusModal, ModalAction};
use cygnus_ui::theme::typography::body_text;
use std::path::PathBuf;

/// Offered export formats.
pub const EXPORT_FORMATS: &[&str] = &["PNG", "JPEG", "JPG", "WEBP", "TIFF", "GIF"];

/// Quiet time after the last format/quality change before asking the
/// worker for a size estimate (a quality drag sends one request).
pub const ESTIMATE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(250);

/// True for the formats carrying a quality setting.
fn has_quality(ext: &str) -> bool {
    matches!(ext, "jpeg" | "jpg")
}

/// Human-readable size (Ko/Mo per language, one decimal above 1 MB).
#[must_use]
pub fn format_bytes(bytes: u64, texts: cygnus_i18n::PhotoCatalog) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b >= KB * KB {
        format!(
            "{:.1} {}",
            b / (KB * KB),
            texts.get(cygnus_i18n::PhotoTextKey::SizeUnitMegabytes)
        )
    } else {
        format!(
            "{} {}",
            (b / KB).ceil() as u64,
            texts.get(cygnus_i18n::PhotoTextKey::SizeUnitKilobytes)
        )
    }
}

/// Validated export request (folder + name + format + quality).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportRequest {
    /// Full destination path (extension = format).
    pub path: PathBuf,
    /// JPEG quality 1..=100.
    pub quality: u8,
}

/// "Export" window state.
#[derive(Debug, Clone)]
pub struct ExportDialogState {
    /// Window visible.
    pub open: bool,
    /// Destination folder (text field).
    pub folder: String,
    /// File name without extension.
    pub filename: String,
    /// Index into [`EXPORT_FORMATS`].
    pub format: usize,
    /// JPEG quality 1..=100.
    pub quality: f32,
    /// Last observed estimate key and when it changed (debounce).
    seen_key: Option<((String, u8), std::time::Instant)>,
    /// Estimate key already requested from the worker.
    requested_key: Option<(String, u8)>,
}

impl Default for ExportDialogState {
    fn default() -> Self {
        Self {
            open: false,
            folder: std::env::temp_dir().display().to_string(),
            filename: String::from("export"),
            format: 0,
            quality: 90.0,
            seen_key: None,
            requested_key: None,
        }
    }
}

impl ExportDialogState {
    /// Opens the export window (the document may have changed since
    /// the last estimate: a fresh one is requested).
    pub fn open(&mut self) {
        self.open = true;
        self.seen_key = None;
        self.requested_key = None;
    }

    /// Lowercase extension of the selected format.
    #[must_use]
    pub fn ext(&self) -> String {
        EXPORT_FORMATS
            .get(self.format)
            .copied()
            .unwrap_or("PNG")
            .to_ascii_lowercase()
    }

    /// (extension, quality) identifying a size estimate; quality is 0
    /// for formats without one (any value gives the same file).
    #[must_use]
    pub fn estimate_key(&self) -> (String, u8) {
        let ext = self.ext();
        let quality = if has_quality(&ext) {
            self.quality.round().clamp(1.0, 100.0) as u8
        } else {
            0
        };
        (ext, quality)
    }

    /// Debounced estimate request: returns the key to send once the
    /// settings stayed still for [`ESTIMATE_DEBOUNCE`] and differ from
    /// the last request. GIF is never estimated.
    pub fn poll_estimate(&mut self, now: std::time::Instant) -> Option<(String, u8)> {
        if !self.open {
            return None;
        }
        let key = self.estimate_key();
        match &self.seen_key {
            Some((seen, _)) if *seen == key => {}
            _ => self.seen_key = Some((key.clone(), now)),
        }
        let still = self
            .seen_key
            .as_ref()
            .is_some_and(|(_, since)| now.duration_since(*since) >= ESTIMATE_DEBOUNCE);
        if !still || key.0 == "gif" || self.requested_key.as_ref() == Some(&key) {
            return None;
        }
        self.requested_key = Some(key.clone());
        Some(key)
    }

    /// True while an estimate for the current settings is still due
    /// (the caller schedules a repaint to fire the debounce).
    #[must_use]
    pub fn estimate_pending(&self) -> bool {
        let key = self.estimate_key();
        self.open && key.0 != "gif" && self.requested_key.as_ref() != Some(&key)
    }

    /// Builds the validated request (sanitized name, extension = format).
    #[must_use]
    pub fn request(&self) -> ExportRequest {
        let ext = self.ext();
        let stem = self.filename.trim();
        let stem = if stem.is_empty() { "export" } else { stem };
        ExportRequest {
            path: PathBuf::from(self.folder.clone()).join(format!("{stem}.{ext}")),
            quality: self.quality.round().clamp(1.0, 100.0) as u8,
        }
    }
}

/// Draws the "Export" window. Returns the validated request
/// (the modal's confirm button). `estimate` = last worker estimate,
/// shown only when it matches the current format/quality.
pub fn draw_export_dialog(
    ui_ctx: &crate::commands::PhotoUiContext,
    state: &mut ExportDialogState,
    estimate: Option<&crate::state::ExportEstimate>,
) -> Option<ExportRequest> {
    if !state.open {
        return None;
    }
    let ctx = ui_ctx.shared.ctx();
    let theme = *ui_ctx.shared.theme();
    let translator = ui_ctx.shared.translator();
    let texts = cygnus_i18n::PhotoCatalog::new(translator.language());
    let mut confirm: Option<ExportRequest> = None;
    let mut open = true;
    let action = CygnusModal::new(
        texts.get(cygnus_i18n::PhotoTextKey::ExportTitle),
        translator.get(cygnus_i18n::TextKey::Validate),
        translator.get(cygnus_i18n::TextKey::Cancel),
    )
    .show(ctx, &mut open, |ui| {
        ui.horizontal(|ui| {
            ui.label(body_text(
                &theme,
                texts.get(cygnus_i18n::PhotoTextKey::FolderLabel),
            ));
            TextInput::new()
                .placeholder(texts.get(cygnus_i18n::PhotoTextKey::FolderPlaceholder))
                .show(ui, &theme, &mut state.folder);
        });
        ui.horizontal(|ui| {
            ui.label(body_text(
                &theme,
                texts.get(cygnus_i18n::PhotoTextKey::NameLabel),
            ));
            TextInput::new()
                .placeholder(texts.get(cygnus_i18n::PhotoTextKey::FileNamePlaceholder))
                .show(ui, &theme, &mut state.filename);
        });
        let formats: Vec<&str> = EXPORT_FORMATS.to_vec();
        Select::new(texts.get(cygnus_i18n::PhotoTextKey::FormatLabel), &formats).show(
            ui,
            &theme,
            &mut state.format,
        );
        let ext = state.ext();
        if has_quality(&ext) {
            Slider::new(
                texts.get(cygnus_i18n::PhotoTextKey::QualityLabel),
                1.0..=100.0,
            )
            .show(ui, &theme, &mut state.quality);
        }
        let note = match ext.as_str() {
            "webp" => Some(cygnus_i18n::PhotoTextKey::WebpLosslessNote),
            "tiff" => Some(cygnus_i18n::PhotoTextKey::TiffNote),
            _ => None,
        };
        if let Some(note) = note {
            ui.label(body_text(&theme, texts.get(note)));
        }
        if ext != "gif" {
            let key = state.estimate_key();
            let size = estimate
                .filter(|e| e.ext == key.0 && e.quality == key.1)
                .map(|e| e.bytes);
            let shown = match size {
                Some(Some(bytes)) => format_bytes(bytes, texts),
                Some(None) => String::from("—"),
                None => texts
                    .get(cygnus_i18n::PhotoTextKey::EstimatingSize)
                    .to_owned(),
            };
            ui.label(body_text(
                &theme,
                &format!(
                    "{} {shown}",
                    texts.get(cygnus_i18n::PhotoTextKey::EstimatedSizeLabel)
                ),
            ));
        }
        let request = state.request();
        ui.label(body_text(
            &theme,
            &format!(
                "{} {}",
                texts.get(cygnus_i18n::PhotoTextKey::DestinationLabel),
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

/// Draws the help window (tools, layers, shortcuts).
pub fn draw_help_dialog(ui_ctx: &crate::commands::PhotoUiContext, open: &mut bool) {
    if !*open {
        return;
    }
    let ctx = ui_ctx.shared.ctx();
    let theme = *ui_ctx.shared.theme();
    let translator = ui_ctx.shared.translator();
    let texts = cygnus_i18n::PhotoCatalog::new(translator.language());
    let mut stays_open = true;
    CygnusModal::new(
        texts.get(cygnus_i18n::PhotoTextKey::HelpTitle),
        translator.get(cygnus_i18n::TextKey::Close),
        translator.get(cygnus_i18n::TextKey::Close),
    )
    .show(ctx, &mut stays_open, |ui| {
        ui.label(body_text(
            &theme,
            texts.get(cygnus_i18n::PhotoTextKey::HelpToolsText),
        ));
        ui.label(body_text(
            &theme,
            texts.get(cygnus_i18n::PhotoTextKey::HelpLayersText),
        ));
        ui.label(body_text(
            &theme,
            texts.get(cygnus_i18n::PhotoTextKey::HelpFileText),
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
        state.format = 3;
        assert_eq!(
            state.request().path,
            PathBuf::from("/tmp/cygnus/export.webp")
        );
        state.format = 4;
        assert_eq!(
            state.request().path,
            PathBuf::from("/tmp/cygnus/export.tiff")
        );
    }

    #[test]
    fn estimate_key_ignores_quality_outside_jpeg() {
        let mut state = ExportDialogState {
            format: 1,
            quality: 70.0,
            ..Default::default()
        };
        assert_eq!(state.estimate_key(), (String::from("jpeg"), 70));
        state.format = 3;
        assert_eq!(state.estimate_key(), (String::from("webp"), 0));
    }

    #[test]
    fn estimate_requests_are_debounced_and_deduplicated() {
        let t0 = std::time::Instant::now();
        let mut state = ExportDialogState::default();
        assert_eq!(state.poll_estimate(t0), None, "fermé : rien");
        state.open();
        assert_eq!(state.poll_estimate(t0), None, "pas encore stable");
        assert!(state.estimate_pending());
        let later = t0 + ESTIMATE_DEBOUNCE;
        assert_eq!(state.poll_estimate(later), Some((String::from("png"), 0)));
        assert_eq!(state.poll_estimate(later), None, "déjà demandé");
        assert!(!state.estimate_pending());
        // Change: new debounce window, then one request.
        state.format = 1;
        assert_eq!(state.poll_estimate(later), None);
        assert_eq!(
            state.poll_estimate(later + ESTIMATE_DEBOUNCE),
            Some((String::from("jpeg"), 90))
        );
        // GIF: never estimated.
        state.format = 5;
        assert_eq!(state.poll_estimate(later + ESTIMATE_DEBOUNCE * 3), None);
        assert!(!state.estimate_pending());
        // Re-opening asks again for the same settings.
        state.format = 0;
        state.open();
        assert_eq!(state.poll_estimate(later), None);
        assert_eq!(
            state.poll_estimate(later + ESTIMATE_DEBOUNCE),
            Some((String::from("png"), 0))
        );
    }

    #[test]
    fn bytes_are_humanized() {
        let fr = cygnus_i18n::PhotoCatalog::new(cygnus_i18n::Language::Fr);
        let en = cygnus_i18n::PhotoCatalog::new(cygnus_i18n::Language::En);
        assert_eq!(format_bytes(1, fr), "1 Ko");
        assert_eq!(format_bytes(2048, en), "2 KB");
        assert_eq!(format_bytes(3 * 1024 * 1024 + 512 * 1024, fr), "3.5 Mo");
    }
}

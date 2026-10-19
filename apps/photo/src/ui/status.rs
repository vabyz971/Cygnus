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

//! Status-bar messages: structured state, translated at display time
//! (the language may change after the message was produced; engine
//! errors arrive language-free, see `photo_engine::session::EngineError`).

use std::path::PathBuf;

use cygnus_i18n::{PhotoCatalog, PhotoTextKey};
use photo_engine::session::EngineError;

/// What the status bar reports for a document.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PhotoStatus {
    /// Nothing to report.
    #[default]
    None,
    /// Document opened, worker ready.
    Ready,
    /// Project save sent to the worker.
    Saving(PathBuf),
    /// Project load sent to the worker.
    Loading(PathBuf),
    /// Export written.
    Exported(PathBuf),
    /// Project saved.
    Saved(PathBuf),
    /// The native file dialog could not be shown.
    PickerUnavailable,
    /// An engine operation failed.
    Error(EngineError),
}

/// "label : value" with the language's separator.
fn labeled(texts: PhotoCatalog, label: PhotoTextKey, value: &str) -> String {
    format!(
        "{}{}{value}",
        texts.get(label),
        texts.get(PhotoTextKey::LabelSeparator)
    )
}

/// Translated message of an engine error.
#[must_use]
pub fn engine_error_message(error: &EngineError, texts: PhotoCatalog) -> String {
    use PhotoTextKey as K;
    match error {
        EngineError::OpenImage { detail } => labeled(texts, K::ErrorOpenImage, detail),
        EngineError::Flip { detail } => labeled(texts, K::ErrorFlip, detail),
        EngineError::Rotate { detail } => labeled(texts, K::ErrorRotate, detail),
        EngineError::CropTransformed => texts.get(K::ErrorCropTransformed).to_owned(),
        EngineError::CropOutside => texts.get(K::ErrorCropOutside).to_owned(),
        EngineError::Crop { detail } => labeled(texts, K::ErrorCrop, detail),
        EngineError::UnknownFilter { type_id } => labeled(texts, K::ErrorUnknownFilter, type_id),
        EngineError::UnknownPreset { preset } => labeled(texts, K::ErrorUnknownPreset, preset),
        EngineError::NothingToExport => texts.get(K::ErrorNothingToExport).to_owned(),
        EngineError::Export { detail } => labeled(texts, K::ErrorExport, detail),
        EngineError::SaveProject { detail } => labeled(texts, K::ErrorSaveProject, detail),
        EngineError::UnsupportedProjectVersion { version } => format!(
            "{} ({})",
            labeled(
                texts,
                K::ErrorUnsupportedProjectVersion,
                &version.to_string()
            ),
            texts.get(K::ErrorProjectVersionHint)
        ),
        EngineError::LoadProject { detail } => labeled(texts, K::ErrorLoadProject, detail),
    }
}

impl PhotoStatus {
    /// Translated message (`None` when there is nothing to show).
    #[must_use]
    pub fn message(&self, texts: PhotoCatalog) -> Option<String> {
        use PhotoTextKey as K;
        let path = |p: &PathBuf| p.display().to_string();
        Some(match self {
            Self::None => return None,
            Self::Ready => texts.get(K::StatusReady).to_owned(),
            Self::Saving(p) => labeled(texts, K::StatusSaving, &path(p)),
            Self::Loading(p) => labeled(texts, K::StatusLoading, &path(p)),
            Self::Exported(p) => labeled(texts, K::StatusExported, &path(p)),
            Self::Saved(p) => labeled(texts, K::StatusSaved, &path(p)),
            Self::PickerUnavailable => texts.get(K::StatusPickerUnavailable).to_owned(),
            Self::Error(error) => engine_error_message(error, texts),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cygnus_i18n::Language;

    #[test]
    fn messages_follow_the_language() {
        let fr = PhotoCatalog::new(Language::Fr);
        let en = PhotoCatalog::new(Language::En);
        let saved = PhotoStatus::Saved(PathBuf::from("/tmp/a.cygp"));
        assert_eq!(
            saved.message(fr).as_deref(),
            Some("Enregistré : /tmp/a.cygp")
        );
        assert_eq!(saved.message(en).as_deref(), Some("Saved: /tmp/a.cygp"));
        assert_eq!(PhotoStatus::None.message(fr), None);
    }

    #[test]
    fn engine_errors_are_translated_with_their_parameters() {
        let fr = PhotoCatalog::new(Language::Fr);
        let en = PhotoCatalog::new(Language::En);
        let unknown = EngineError::UnknownPreset {
            preset: String::from("x.y"),
        };
        assert_eq!(
            engine_error_message(&unknown, fr),
            "Préréglage inconnu : x.y"
        );
        assert_eq!(engine_error_message(&unknown, en), "Unknown preset: x.y");
        let version = EngineError::UnsupportedProjectVersion { version: 1 };
        assert!(engine_error_message(&version, fr).contains("non supportée : 1"));
        assert_eq!(
            engine_error_message(&EngineError::CropOutside, en),
            "Cannot crop: the layer is outside the document"
        );
    }
}

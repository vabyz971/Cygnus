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

//! Textes propres à l'app Photo (barre de menus…).
//!
//! Même principe que [`ui_kit::i18n`] : clés stables
//! ([`PhotoTextKey`]) + tables fr/en (`&'static str`, zéro
//! allocation) via [`PhotoCatalog`]. Les libellés génériques
//! (Fichier, Annuler…) restent dans ui-kit ; ici vivent UNIQUEMENT
//! les chaînes métier de Photo. ui-kit ne reçoit aucun texte métier.

use ui_kit::i18n::Language;

/// Clé de texte stable propre à Photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhotoTextKey {
    /// Fermer le document actif.
    CloseDocument,
    /// Ouvrir un projet `.cygp` (nouvel onglet).
    OpenProject,
    /// Enregistrer sous (projet `.cygp`).
    SaveAs,
    /// Nouveau calque vide.
    NewEmptyLayer,
    /// Calque depuis une image.
    LayerFromImage,
    /// Dupliquer le calque.
    DuplicateLayer,
    /// Miroir horizontal (O004).
    FlipHorizontal,
    /// Miroir vertical (O004).
    FlipVertical,
    /// Rotation 90° horaire (O004).
    RotateClockwise,
    /// Rotation 90° antihoraire (O004).
    RotateCounterclockwise,
    /// Rogner le calque au document (O004).
    CropToDocument,
    /// Ajouter un masque.
    AddMask,
    /// Supprimer le calque.
    DeleteLayer,
    /// Rogner l'aperçu au document (coche `[x]`/`[ ]` ajoutée par l'appelant).
    CropPreviewToDocument,
    /// Zoom 100 %.
    Zoom100,
    /// Réinitialiser la disposition des docks.
    ResetLayout,
    /// Panneau Historique vide.
    HistoryEmpty,
    /// À propos.
    About,
}

/// Catalogue photo lié à une langue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhotoCatalog {
    lang: Language,
}

impl PhotoCatalog {
    /// Catalogue pour la langue `lang`.
    pub fn new(lang: Language) -> Self {
        Self { lang }
    }

    /// Traduit `key` dans la langue du catalogue.
    pub fn get(self, key: PhotoTextKey) -> &'static str {
        match self.lang {
            Language::En => translate_en(key),
            Language::Fr => translate_fr(key),
        }
    }
}

/// Table française (rendu historique, inchangé).
fn translate_fr(key: PhotoTextKey) -> &'static str {
    match key {
        PhotoTextKey::CloseDocument => "Fermer le document",
        PhotoTextKey::OpenProject => "Ouvrir un projet",
        PhotoTextKey::SaveAs => "Enregistrer sous",
        PhotoTextKey::NewEmptyLayer => "Nouveau calque vide",
        PhotoTextKey::LayerFromImage => "Calque depuis une image",
        PhotoTextKey::DuplicateLayer => "Dupliquer le calque",
        PhotoTextKey::FlipHorizontal => "Miroir horizontal",
        PhotoTextKey::FlipVertical => "Miroir vertical",
        PhotoTextKey::RotateClockwise => "Rotation 90° horaire",
        PhotoTextKey::RotateCounterclockwise => "Rotation 90° antihoraire",
        PhotoTextKey::CropToDocument => "Rogner au document",
        PhotoTextKey::AddMask => "Ajouter un masque",
        PhotoTextKey::DeleteLayer => "Supprimer le calque",
        PhotoTextKey::CropPreviewToDocument => "Rogner l'apercu au document",
        PhotoTextKey::Zoom100 => "Zoom 100 %",
        PhotoTextKey::ResetLayout => "Réinitialiser la disposition",
        PhotoTextKey::HistoryEmpty => "Aucune modification",
        PhotoTextKey::About => "À propos",
    }
}

/// Table anglaise.
fn translate_en(key: PhotoTextKey) -> &'static str {
    match key {
        PhotoTextKey::CloseDocument => "Close document",
        PhotoTextKey::OpenProject => "Open project",
        PhotoTextKey::SaveAs => "Save as",
        PhotoTextKey::NewEmptyLayer => "New empty layer",
        PhotoTextKey::LayerFromImage => "Layer from image",
        PhotoTextKey::DuplicateLayer => "Duplicate layer",
        PhotoTextKey::FlipHorizontal => "Flip horizontal",
        PhotoTextKey::FlipVertical => "Flip vertical",
        PhotoTextKey::RotateClockwise => "Rotate 90° clockwise",
        PhotoTextKey::RotateCounterclockwise => "Rotate 90° counterclockwise",
        PhotoTextKey::CropToDocument => "Crop to document",
        PhotoTextKey::AddMask => "Add mask",
        PhotoTextKey::DeleteLayer => "Delete layer",
        PhotoTextKey::CropPreviewToDocument => "Crop preview to document",
        PhotoTextKey::Zoom100 => "Zoom 100%",
        PhotoTextKey::ResetLayout => "Reset layout",
        PhotoTextKey::HistoryEmpty => "No changes yet",
        PhotoTextKey::About => "About",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le français garde le rendu historique au caractère près
    /// (y compris `apercu` sans accent et l'espace de `100 %`).
    #[test]
    fn french_matches_legacy_menu_strings() {
        let texts = PhotoCatalog::new(Language::Fr);
        assert_eq!(texts.get(PhotoTextKey::CloseDocument), "Fermer le document");
        assert_eq!(texts.get(PhotoTextKey::OpenProject), "Ouvrir un projet");
        assert_eq!(texts.get(PhotoTextKey::SaveAs), "Enregistrer sous");
        assert_eq!(
            texts.get(PhotoTextKey::NewEmptyLayer),
            "Nouveau calque vide"
        );
        assert_eq!(
            texts.get(PhotoTextKey::LayerFromImage),
            "Calque depuis une image"
        );
        assert_eq!(
            texts.get(PhotoTextKey::DuplicateLayer),
            "Dupliquer le calque"
        );
        assert_eq!(texts.get(PhotoTextKey::FlipHorizontal), "Miroir horizontal");
        assert_eq!(texts.get(PhotoTextKey::FlipVertical), "Miroir vertical");
        assert_eq!(
            texts.get(PhotoTextKey::RotateClockwise),
            "Rotation 90° horaire"
        );
        assert_eq!(
            texts.get(PhotoTextKey::RotateCounterclockwise),
            "Rotation 90° antihoraire"
        );
        assert_eq!(
            texts.get(PhotoTextKey::CropToDocument),
            "Rogner au document"
        );
        assert_eq!(texts.get(PhotoTextKey::AddMask), "Ajouter un masque");
        assert_eq!(texts.get(PhotoTextKey::DeleteLayer), "Supprimer le calque");
        assert_eq!(
            texts.get(PhotoTextKey::CropPreviewToDocument),
            "Rogner l'apercu au document"
        );
        assert_eq!(texts.get(PhotoTextKey::Zoom100), "Zoom 100 %");
        assert_eq!(
            texts.get(PhotoTextKey::ResetLayout),
            "Réinitialiser la disposition"
        );
        assert_eq!(texts.get(PhotoTextKey::HistoryEmpty), "Aucune modification");
        assert_eq!(texts.get(PhotoTextKey::About), "À propos");
    }

    #[test]
    fn english_translates_every_key() {
        let texts = PhotoCatalog::new(Language::En);
        for key in [
            PhotoTextKey::CloseDocument,
            PhotoTextKey::OpenProject,
            PhotoTextKey::SaveAs,
            PhotoTextKey::NewEmptyLayer,
            PhotoTextKey::LayerFromImage,
            PhotoTextKey::DuplicateLayer,
            PhotoTextKey::FlipHorizontal,
            PhotoTextKey::FlipVertical,
            PhotoTextKey::RotateClockwise,
            PhotoTextKey::RotateCounterclockwise,
            PhotoTextKey::CropToDocument,
            PhotoTextKey::AddMask,
            PhotoTextKey::DeleteLayer,
            PhotoTextKey::CropPreviewToDocument,
            PhotoTextKey::Zoom100,
            PhotoTextKey::ResetLayout,
            PhotoTextKey::HistoryEmpty,
            PhotoTextKey::About,
        ] {
            assert!(!texts.get(key).is_empty(), "clé non traduite : {key:?}");
        }
        assert_eq!(texts.get(PhotoTextKey::CloseDocument), "Close document");
    }
}

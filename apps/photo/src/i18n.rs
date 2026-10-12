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

//! Photo-specific strings (menu bar…).
//!
//! Same principle as [`ui_kit::i18n`]: stable keys
//! ([`PhotoTextKey`]) + fr/en tables (`&'static str`, zero
//! allocation) via [`PhotoCatalog`]. Generic labels
//! (File, Undo…) stay in ui-kit; ONLY Photo's
//! domain strings live here. ui-kit receives no domain text.

use ui_kit::i18n::Language;

/// Stable text key specific to Photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PhotoTextKey {
    /// Close the active document.
    CloseDocument,
    /// Open a `.cygp` project (new tab).
    OpenProject,
    /// Save as (`.cygp` project).
    SaveAs,
    /// New empty layer.
    NewEmptyLayer,
    /// Layer from an image.
    LayerFromImage,
    /// Duplicate the layer.
    DuplicateLayer,
    /// Horizontal mirror (O004).
    FlipHorizontal,
    /// Vertical mirror (O004).
    FlipVertical,
    /// 90° clockwise rotation (O004).
    RotateClockwise,
    /// 90° counterclockwise rotation (O004).
    RotateCounterclockwise,
    /// Crop the layer to the document (O004).
    CropToDocument,
    /// Transformations submenu (mirror, rotate, crop).
    Transform,
    /// Expand a group (tree chevron).
    ExpandGroup,
    /// Collapse a group.
    CollapseGroup,
    /// Expand a layer's attachments.
    ExpandAttachments,
    /// Collapse the attachments.
    CollapseAttachments,
    /// Enable / disable the filter (eye).
    ToggleFilterEnabled,
    /// Enable / disable the mask (eye).
    ToggleMaskEnabled,
    /// Delete the filter.
    DeleteFilter,
    /// Delete the mask.
    DeleteMask,
    /// Show / hide (layer eye).
    ShowHideLayer,
    /// Solo a layer (isolate it).
    IsolateLayer,
    /// Empty list (layers header).
    NoLayers,
    /// No layer selected (inspector).
    NoLayerSelected,
    /// "Live filter" label (inspector).
    FilterLiveLabel,
    /// "Mask" label (inspector).
    MaskLabel,
    /// "Filter enabled" switch.
    FilterActive,
    /// "Mask enabled" switch.
    MaskActive,
    /// "Brightness" slider (filter).
    FilterParamBrightness,
    /// "Contrast" slider (filter).
    FilterParamContrast,
    /// "Radius" slider (filter).
    FilterParamRadius,
    /// "Saturation" slider (filter).
    FilterParamSaturation,
    /// "Hue" slider (filter).
    FilterParamHue,
    /// "Opacity" label.
    OpacityLabel,
    /// "Blend" label.
    BlendLabel,
    /// "Filter list" tooltip.
    FilterList,
    /// "Open an image…" button.
    OpenImageFile,
    /// Move tool title.
    MoveHint,
    /// Hand tool title.
    PanHint,
    /// Zoom tool title.
    ZoomHint,
    /// Brush tool title.
    BrushHint,
    /// Eraser tool title.
    EraserHint,
    /// Eyedropper tool title.
    EyedropperHint,
    /// Move contextual help (status bar).
    MoveHintText,
    /// Hand contextual help.
    PanHintText,
    /// Zoom contextual help.
    ZoomHintText,
    /// Brush contextual help.
    BrushHintText,
    /// Eraser contextual help.
    EraserHintText,
    /// Eyedropper contextual help.
    EyedropperHintText,
    /// "Size" label.
    SizeLabel,
    /// "Color" label.
    ColorLabel,
    /// "Move" title (options bar).
    MoveTitle,
    /// "Grid" label.
    GridLabel,
    /// Welcome screen: no document.
    NoDocumentOpen,
    /// Welcome screen: invitation text.
    WelcomeBody,
    /// Empty dock panel.
    NoDocument,
    /// "Create document" title.
    CreateDocumentTitle,
    /// "Print" tab.
    PrintTab,
    /// "Digital" tab.
    DigitalTab,
    /// "Unit" label.
    UnitLabel,
    /// "Width" label.
    WidthLabel,
    /// "Height" label.
    HeightLabel,
    /// "Orientation" label.
    OrientationLabel,
    /// "Portrait" button.
    PortraitLabel,
    /// "Landscape" button.
    LandscapeLabel,
    /// "A4 Portrait" preset.
    PresetA4Portrait,
    /// "A4 Landscape" preset.
    PresetA4Landscape,
    /// "Letter Portrait" preset.
    PresetLetterPortrait,
    /// "Letter Landscape" preset.
    PresetLetterLandscape,
    /// "Square" preset.
    PresetSquare,
    /// "4K UHD" preset (invariant).
    Preset4kUhd,
    /// "8K UHD" preset (invariant).
    Preset8kUhd,
    /// "16:9 Full HD" preset (invariant).
    Preset16by9FullHd,
    /// "Export" title.
    ExportTitle,
    /// "Folder" label.
    FolderLabel,
    /// "Export folder" placeholder.
    FolderPlaceholder,
    /// "Name" label.
    NameLabel,
    /// "File name" placeholder.
    FileNamePlaceholder,
    /// "Format" label.
    FormatLabel,
    /// "Quality" label.
    QualityLabel,
    /// "Destination:" prefix.
    DestinationLabel,
    /// "Photo help" title.
    HelpTitle,
    /// Help: tools paragraph.
    HelpToolsText,
    /// Help: layers paragraph.
    HelpLayersText,
    /// "Pixels" kind (inspector).
    LayerKindPixel,
    /// "Group" kind.
    LayerKindGroup,
    /// "Adjustment" kind.
    LayerKindAdjustment,
    /// Help: file/export paragraph.
    HelpFileText,
    /// Add a mask.
    AddMask,
    /// Delete the layer.
    DeleteLayer,
    /// Clip the preview to the document (`[x]`/`[ ]` check added by the caller).
    CropPreviewToDocument,
    /// Zoom 100%.
    Zoom100,
    /// Reset the dock layout.
    ResetLayout,
    /// Empty History panel.
    HistoryEmpty,
    /// Opening-state row (undo everything).
    HistoryInitial,
    /// About.
    About,
}

/// Photo catalog bound to a language.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhotoCatalog {
    lang: Language,
}

/// Every key (update on each addition: the
/// `all_keys_translated` test requires it non-empty in both languages).
#[cfg(test)]
pub(crate) const ALL_PHOTO_TEXT_KEYS: &[PhotoTextKey] = &[
    PhotoTextKey::CloseDocument,
    PhotoTextKey::NewEmptyLayer,
    PhotoTextKey::LayerFromImage,
    PhotoTextKey::DuplicateLayer,
    PhotoTextKey::AddMask,
    PhotoTextKey::DeleteLayer,
    PhotoTextKey::CropPreviewToDocument,
    PhotoTextKey::Zoom100,
    PhotoTextKey::ResetLayout,
    PhotoTextKey::About,
    PhotoTextKey::OpenProject,
    PhotoTextKey::SaveAs,
    PhotoTextKey::HistoryEmpty,
    PhotoTextKey::HistoryInitial,
    PhotoTextKey::FlipHorizontal,
    PhotoTextKey::FlipVertical,
    PhotoTextKey::RotateClockwise,
    PhotoTextKey::RotateCounterclockwise,
    PhotoTextKey::CropToDocument,
    PhotoTextKey::Transform,
    PhotoTextKey::ExpandGroup,
    PhotoTextKey::CollapseGroup,
    PhotoTextKey::ExpandAttachments,
    PhotoTextKey::CollapseAttachments,
    PhotoTextKey::ToggleFilterEnabled,
    PhotoTextKey::ToggleMaskEnabled,
    PhotoTextKey::DeleteFilter,
    PhotoTextKey::DeleteMask,
    PhotoTextKey::ShowHideLayer,
    PhotoTextKey::IsolateLayer,
    PhotoTextKey::NoLayers,
    PhotoTextKey::NoLayerSelected,
    PhotoTextKey::FilterLiveLabel,
    PhotoTextKey::MaskLabel,
    PhotoTextKey::FilterActive,
    PhotoTextKey::MaskActive,
    PhotoTextKey::FilterParamBrightness,
    PhotoTextKey::FilterParamContrast,
    PhotoTextKey::FilterParamRadius,
    PhotoTextKey::FilterParamSaturation,
    PhotoTextKey::FilterParamHue,
    PhotoTextKey::OpacityLabel,
    PhotoTextKey::BlendLabel,
    PhotoTextKey::FilterList,
    PhotoTextKey::OpenImageFile,
    PhotoTextKey::MoveHint,
    PhotoTextKey::PanHint,
    PhotoTextKey::ZoomHint,
    PhotoTextKey::BrushHint,
    PhotoTextKey::EraserHint,
    PhotoTextKey::EyedropperHint,
    PhotoTextKey::MoveHintText,
    PhotoTextKey::PanHintText,
    PhotoTextKey::ZoomHintText,
    PhotoTextKey::BrushHintText,
    PhotoTextKey::EraserHintText,
    PhotoTextKey::EyedropperHintText,
    PhotoTextKey::SizeLabel,
    PhotoTextKey::ColorLabel,
    PhotoTextKey::MoveTitle,
    PhotoTextKey::GridLabel,
    PhotoTextKey::NoDocumentOpen,
    PhotoTextKey::WelcomeBody,
    PhotoTextKey::NoDocument,
    PhotoTextKey::CreateDocumentTitle,
    PhotoTextKey::PrintTab,
    PhotoTextKey::DigitalTab,
    PhotoTextKey::UnitLabel,
    PhotoTextKey::WidthLabel,
    PhotoTextKey::HeightLabel,
    PhotoTextKey::OrientationLabel,
    PhotoTextKey::PortraitLabel,
    PhotoTextKey::LandscapeLabel,
    PhotoTextKey::PresetA4Portrait,
    PhotoTextKey::PresetA4Landscape,
    PhotoTextKey::PresetLetterPortrait,
    PhotoTextKey::PresetLetterLandscape,
    PhotoTextKey::PresetSquare,
    PhotoTextKey::Preset4kUhd,
    PhotoTextKey::Preset8kUhd,
    PhotoTextKey::Preset16by9FullHd,
    PhotoTextKey::ExportTitle,
    PhotoTextKey::FolderLabel,
    PhotoTextKey::FolderPlaceholder,
    PhotoTextKey::NameLabel,
    PhotoTextKey::FileNamePlaceholder,
    PhotoTextKey::FormatLabel,
    PhotoTextKey::QualityLabel,
    PhotoTextKey::DestinationLabel,
    PhotoTextKey::HelpTitle,
    PhotoTextKey::HelpToolsText,
    PhotoTextKey::HelpLayersText,
    PhotoTextKey::HelpFileText,
    PhotoTextKey::LayerKindPixel,
    PhotoTextKey::LayerKindGroup,
    PhotoTextKey::LayerKindAdjustment,
];

impl PhotoCatalog {
    /// Catalog for language `lang`.
    pub fn new(lang: Language) -> Self {
        Self { lang }
    }

    /// Translates `key` into the catalog language.
    pub fn get(self, key: PhotoTextKey) -> &'static str {
        match self.lang {
            Language::En => translate_en(key),
            Language::Fr => translate_fr(key),
        }
    }
}

/// French table (historical rendering, unchanged).
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
        PhotoTextKey::Transform => "Transformation",
        PhotoTextKey::ExpandGroup => "Déplier",
        PhotoTextKey::CollapseGroup => "Replier",
        PhotoTextKey::ExpandAttachments => "Déplier les effets",
        PhotoTextKey::CollapseAttachments => "Replier les effets",
        PhotoTextKey::ToggleFilterEnabled => "Activer / désactiver le filtre",
        PhotoTextKey::ToggleMaskEnabled => "Activer / désactiver le masque",
        PhotoTextKey::DeleteFilter => "Supprimer le filtre",
        PhotoTextKey::DeleteMask => "Supprimer le masque",
        PhotoTextKey::ShowHideLayer => "Afficher / masquer",
        PhotoTextKey::IsolateLayer => "Solo",
        PhotoTextKey::NoLayers => "Aucun calque",
        PhotoTextKey::NoLayerSelected => "Aucun calque selectionne",
        PhotoTextKey::FilterLiveLabel => "Filtre live",
        PhotoTextKey::MaskLabel => "Masque",
        PhotoTextKey::FilterActive => "Filtre actif",
        PhotoTextKey::MaskActive => "Masque actif",
        PhotoTextKey::FilterParamBrightness => "Luminosité",
        PhotoTextKey::FilterParamContrast => "Contraste",
        PhotoTextKey::FilterParamRadius => "Rayon",
        PhotoTextKey::FilterParamSaturation => "Saturation",
        PhotoTextKey::FilterParamHue => "Teinte",
        PhotoTextKey::OpacityLabel => "Opacité",
        PhotoTextKey::BlendLabel => "Fusion",
        PhotoTextKey::FilterList => "Liste des filtres",
        PhotoTextKey::OpenImageFile => "Ouvrir une image…",
        PhotoTextKey::MoveHint => "Deplacer / selectionner",
        PhotoTextKey::PanHint => "Main (deplacer la vue)",
        PhotoTextKey::ZoomHint => "Loupe",
        PhotoTextKey::BrushHint => "Pinceau",
        PhotoTextKey::EraserHint => "Gomme",
        PhotoTextKey::EyedropperHint => "Pipette",
        PhotoTextKey::MoveHintText => "Glisser : deplacer le calque — molette : zoom",
        PhotoTextKey::PanHintText => "Glisser : deplacer la vue — molette : zoom",
        PhotoTextKey::ZoomHintText => "Clic : zoom avant — clic droit : zoom arriere",
        PhotoTextKey::BrushHintText => "Glisser : peindre — relacher : commettre le trait",
        PhotoTextKey::EraserHintText => "Glisser : effacer — relacher : commettre",
        PhotoTextKey::EyedropperHintText => "Cliquer : echantillonner la couleur",
        PhotoTextKey::SizeLabel => "Taille",
        PhotoTextKey::ColorLabel => "Couleur",
        PhotoTextKey::MoveTitle => "Deplacement",
        PhotoTextKey::GridLabel => "Grille",
        PhotoTextKey::NoDocumentOpen => "Aucun document ouvert",
        PhotoTextKey::WelcomeBody => "Créez un document ou ouvrez une image pour commencer.",
        PhotoTextKey::NoDocument => "Aucun document",
        PhotoTextKey::CreateDocumentTitle => "Créer un document",
        PhotoTextKey::PrintTab => "Impression",
        PhotoTextKey::DigitalTab => "Numérique",
        PhotoTextKey::UnitLabel => "Unité",
        PhotoTextKey::WidthLabel => "Largeur",
        PhotoTextKey::HeightLabel => "Hauteur",
        PhotoTextKey::OrientationLabel => "Orientation",
        PhotoTextKey::PortraitLabel => "Portrait",
        PhotoTextKey::LandscapeLabel => "Paysage",
        PhotoTextKey::PresetA4Portrait => "A4 Portrait",
        PhotoTextKey::PresetA4Landscape => "A4 Paysage",
        PhotoTextKey::PresetLetterPortrait => "Lettre Portrait",
        PhotoTextKey::PresetLetterLandscape => "Lettre Paysage",
        PhotoTextKey::PresetSquare => "Carré",
        PhotoTextKey::Preset4kUhd => "4K UHD",
        PhotoTextKey::Preset8kUhd => "8K UHD",
        PhotoTextKey::Preset16by9FullHd => "16:9 Full HD",
        PhotoTextKey::ExportTitle => "Exportation",
        PhotoTextKey::FolderLabel => "Dossier",
        PhotoTextKey::FolderPlaceholder => "Dossier d'exportation",
        PhotoTextKey::NameLabel => "Nom",
        PhotoTextKey::FileNamePlaceholder => "Nom du fichier",
        PhotoTextKey::FormatLabel => "Format",
        PhotoTextKey::QualityLabel => "Qualité",
        PhotoTextKey::DestinationLabel => "Destination :",
        PhotoTextKey::HelpTitle => "Aide de Photo",
        PhotoTextKey::HelpToolsText => {
            "Outils : deplacement, main, loupe, pinceau, gomme, pipette."
        }
        PhotoTextKey::HelpLayersText => {
            "Calques : clic = selection, double-clic sur le nom = renommer, glisser-deposer = reordonner. Bas du panneau : image, vide, dupliquer, masque, filtres, supprimer."
        }
        PhotoTextKey::HelpFileText => {
            "Fichier > Nouveau document : format, dimensions (px, in, cm, pica), orientation portrait/paysage. Exportation : dossier, format PNG/JPEG/JPG/GIF, validation."
        }
        PhotoTextKey::LayerKindPixel => "Pixels",
        PhotoTextKey::LayerKindGroup => "Groupe",
        PhotoTextKey::LayerKindAdjustment => "Reglage",
        PhotoTextKey::AddMask => "Ajouter un masque",
        PhotoTextKey::DeleteLayer => "Supprimer le calque",
        PhotoTextKey::CropPreviewToDocument => "Rogner l'apercu au document",
        PhotoTextKey::Zoom100 => "Zoom 100 %",
        PhotoTextKey::ResetLayout => "Réinitialiser la disposition",
        PhotoTextKey::HistoryEmpty => "Aucune modification",
        PhotoTextKey::HistoryInitial => "État initial",
        PhotoTextKey::About => "À propos",
    }
}

/// English table.
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
        PhotoTextKey::Transform => "Transform",
        PhotoTextKey::ExpandGroup => "Expand",
        PhotoTextKey::CollapseGroup => "Collapse",
        PhotoTextKey::ExpandAttachments => "Expand effects",
        PhotoTextKey::CollapseAttachments => "Collapse effects",
        PhotoTextKey::ToggleFilterEnabled => "Enable / disable filter",
        PhotoTextKey::ToggleMaskEnabled => "Enable / disable mask",
        PhotoTextKey::DeleteFilter => "Delete filter",
        PhotoTextKey::DeleteMask => "Delete mask",
        PhotoTextKey::ShowHideLayer => "Show / hide",
        PhotoTextKey::IsolateLayer => "Solo",
        PhotoTextKey::NoLayers => "No layers",
        PhotoTextKey::NoLayerSelected => "No layer selected",
        PhotoTextKey::FilterLiveLabel => "Live filter",
        PhotoTextKey::MaskLabel => "Mask",
        PhotoTextKey::FilterActive => "Filter enabled",
        PhotoTextKey::MaskActive => "Mask enabled",
        PhotoTextKey::FilterParamBrightness => "Brightness",
        PhotoTextKey::FilterParamContrast => "Contrast",
        PhotoTextKey::FilterParamRadius => "Radius",
        PhotoTextKey::FilterParamSaturation => "Saturation",
        PhotoTextKey::FilterParamHue => "Hue",
        PhotoTextKey::OpacityLabel => "Opacity",
        PhotoTextKey::BlendLabel => "Blend",
        PhotoTextKey::FilterList => "Filter list",
        PhotoTextKey::OpenImageFile => "Open an image…",
        PhotoTextKey::MoveHint => "Move / select",
        PhotoTextKey::PanHint => "Hand (pan view)",
        PhotoTextKey::ZoomHint => "Zoom",
        PhotoTextKey::BrushHint => "Brush",
        PhotoTextKey::EraserHint => "Eraser",
        PhotoTextKey::EyedropperHint => "Eyedropper",
        PhotoTextKey::MoveHintText => "Drag: move layer — wheel: zoom",
        PhotoTextKey::PanHintText => "Drag: pan view — wheel: zoom",
        PhotoTextKey::ZoomHintText => "Click: zoom in — right-click: zoom out",
        PhotoTextKey::BrushHintText => "Drag: paint — release: commit stroke",
        PhotoTextKey::EraserHintText => "Drag: erase — release: commit",
        PhotoTextKey::EyedropperHintText => "Click canvas to sample color",
        PhotoTextKey::SizeLabel => "Size",
        PhotoTextKey::ColorLabel => "Color",
        PhotoTextKey::MoveTitle => "Move",
        PhotoTextKey::GridLabel => "Grid",
        PhotoTextKey::NoDocumentOpen => "No open document",
        PhotoTextKey::WelcomeBody => "Create a document or open an image to start.",
        PhotoTextKey::NoDocument => "No document",
        PhotoTextKey::CreateDocumentTitle => "New document",
        PhotoTextKey::PrintTab => "Print",
        PhotoTextKey::DigitalTab => "Digital",
        PhotoTextKey::UnitLabel => "Unit",
        PhotoTextKey::WidthLabel => "Width",
        PhotoTextKey::HeightLabel => "Height",
        PhotoTextKey::OrientationLabel => "Orientation",
        PhotoTextKey::PortraitLabel => "Portrait",
        PhotoTextKey::LandscapeLabel => "Landscape",
        PhotoTextKey::PresetA4Portrait => "A4 Portrait",
        PhotoTextKey::PresetA4Landscape => "A4 Landscape",
        PhotoTextKey::PresetLetterPortrait => "Letter Portrait",
        PhotoTextKey::PresetLetterLandscape => "Letter Landscape",
        PhotoTextKey::PresetSquare => "Square",
        PhotoTextKey::Preset4kUhd => "4K UHD",
        PhotoTextKey::Preset8kUhd => "8K UHD",
        PhotoTextKey::Preset16by9FullHd => "16:9 Full HD",
        PhotoTextKey::ExportTitle => "Export",
        PhotoTextKey::FolderLabel => "Folder",
        PhotoTextKey::FolderPlaceholder => "Export folder",
        PhotoTextKey::NameLabel => "Name",
        PhotoTextKey::FileNamePlaceholder => "File name",
        PhotoTextKey::FormatLabel => "Format",
        PhotoTextKey::QualityLabel => "Quality",
        PhotoTextKey::DestinationLabel => "Destination:",
        PhotoTextKey::HelpTitle => "Photo help",
        PhotoTextKey::HelpToolsText => "Tools: move, hand, zoom, brush, eraser, eyedropper.",
        PhotoTextKey::HelpLayersText => {
            "Layers: click = select, double-click name = rename, drag and drop = reorder. Panel bottom: image, empty, duplicate, mask, filters, delete."
        }
        PhotoTextKey::HelpFileText => {
            "File > New document: format, dimensions (px, in, cm, pica), portrait/landscape orientation. Export: folder, PNG/JPEG/JPG/GIF format, confirm."
        }
        PhotoTextKey::LayerKindPixel => "Pixels",
        PhotoTextKey::LayerKindGroup => "Group",
        PhotoTextKey::LayerKindAdjustment => "Adjustment",
        PhotoTextKey::AddMask => "Add mask",
        PhotoTextKey::DeleteLayer => "Delete layer",
        PhotoTextKey::CropPreviewToDocument => "Crop preview to document",
        PhotoTextKey::Zoom100 => "Zoom 100%",
        PhotoTextKey::ResetLayout => "Reset layout",
        PhotoTextKey::HistoryEmpty => "No changes yet",
        PhotoTextKey::HistoryInitial => "Initial state",
        PhotoTextKey::About => "About",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// French keeps the historical rendering character-for-character
    /// (including accent-less `apercu` and the `100 %` space).
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
        assert_eq!(texts.get(PhotoTextKey::Transform), "Transformation");
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
        for key in ALL_PHOTO_TEXT_KEYS {
            assert!(!texts.get(*key).is_empty(), "clé non traduite : {key:?}");
        }
        assert_eq!(texts.get(PhotoTextKey::CloseDocument), "Close document");
    }

    #[test]
    fn all_keys_translated_in_both_languages() {
        for lang in [Language::Fr, Language::En] {
            let texts = PhotoCatalog::new(lang);
            for key in ALL_PHOTO_TEXT_KEYS {
                assert!(
                    !texts.get(*key).is_empty(),
                    "clé vide : {key:?} en {lang:?}"
                );
            }
        }
    }
}

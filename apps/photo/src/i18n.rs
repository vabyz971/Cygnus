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
    /// Sous-menu des transformations (miroir, rotation, crop).
    Transform,
    /// Déplier un groupe (chevron de l'arbre).
    ExpandGroup,
    /// Replier un groupe.
    CollapseGroup,
    /// Déplier les pièces jointes d'un calque.
    ExpandAttachments,
    /// Replier les pièces jointes.
    CollapseAttachments,
    /// Activer / désactiver le filtre (œil).
    ToggleFilterEnabled,
    /// Activer / désactiver le masque (œil).
    ToggleMaskEnabled,
    /// Monter le filtre.
    MoveFilterUp,
    /// Descendre le filtre.
    MoveFilterDown,
    /// Supprimer le filtre.
    DeleteFilter,
    /// Monter le masque.
    MoveMaskUp,
    /// Descendre le masque.
    MoveMaskDown,
    /// Supprimer le masque.
    DeleteMask,
    /// Afficher / masquer (œil du calque).
    ShowHideLayer,
    /// Liste vide (en-tête calques).
    NoLayers,
    /// Aucun calque sélectionné (inspecteur).
    NoLayerSelected,
    /// Libellé « Filtres » (compteur inspecteur).
    FiltersLabel,
    /// Libellé « Masques » (compteur inspecteur).
    MasksLabel,
    /// Libellé « Filtre live » (inspecteur).
    FilterLiveLabel,
    /// Libellé « Masque » (inspecteur).
    MaskLabel,
    /// Interrupteur « Filtre actif ».
    FilterActive,
    /// Interrupteur « Masque actif ».
    MaskActive,
    /// Label « Visible » (en-tête calques).
    VisibleLabel,
    /// Label « Opacité ».
    OpacityLabel,
    /// Label « Fusion ».
    BlendLabel,
    /// Label « Filtre » (modale).
    FilterLabel,
    /// Infobulle « Liste des filtres ».
    FilterList,
    /// Titre « Ajouter un filtre » (modale).
    AddFilterTitle,
    /// Bouton « Ouvrir une image… ».
    OpenImageFile,
    /// Titre de l'outil Déplacement.
    MoveHint,
    /// Titre de l'outil Main.
    PanHint,
    /// Titre de l'outil Loupe.
    ZoomHint,
    /// Titre de l'outil Pinceau.
    BrushHint,
    /// Titre de l'outil Gomme.
    EraserHint,
    /// Titre de l'outil Pipette.
    EyedropperHint,
    /// Aide contextuelle Déplacement (barre de statut).
    MoveHintText,
    /// Aide contextuelle Main.
    PanHintText,
    /// Aide contextuelle Loupe.
    ZoomHintText,
    /// Aide contextuelle Pinceau.
    BrushHintText,
    /// Aide contextuelle Gomme.
    EraserHintText,
    /// Aide contextuelle Pipette.
    EyedropperHintText,
    /// Label « Taille ».
    SizeLabel,
    /// Label « Couleur ».
    ColorLabel,
    /// Titre « Déplacement » (barre d'options).
    MoveTitle,
    /// Label « Grille ».
    GridLabel,
    /// Écran d'accueil : aucun document.
    NoDocumentOpen,
    /// Écran d'accueil : texte d'invitation.
    WelcomeBody,
    /// Panneau dock vide.
    NoDocument,
    /// Titre « Créer un document ».
    CreateDocumentTitle,
    /// Onglet « Impression ».
    PrintTab,
    /// Onglet « Numérique ».
    DigitalTab,
    /// Label « Unité ».
    UnitLabel,
    /// Label « Largeur ».
    WidthLabel,
    /// Label « Hauteur ».
    HeightLabel,
    /// Label « Orientation ».
    OrientationLabel,
    /// Bouton « Portrait ».
    PortraitLabel,
    /// Bouton « Paysage ».
    LandscapeLabel,
    /// Préset « A4 Portrait ».
    PresetA4Portrait,
    /// Préset « A4 Paysage ».
    PresetA4Landscape,
    /// Préset « Lettre Portrait ».
    PresetLetterPortrait,
    /// Préset « Lettre Paysage ».
    PresetLetterLandscape,
    /// Préset « Carré ».
    PresetSquare,
    /// Préset « 4K UHD » (invariable).
    Preset4kUhd,
    /// Préset « 8K UHD » (invariable).
    Preset8kUhd,
    /// Préset « 16:9 Full HD » (invariable).
    Preset16by9FullHd,
    /// Titre « Exportation ».
    ExportTitle,
    /// Label « Dossier ».
    FolderLabel,
    /// Placeholder « Dossier d'exportation ».
    FolderPlaceholder,
    /// Label « Nom ».
    NameLabel,
    /// Placeholder « Nom du fichier ».
    FileNamePlaceholder,
    /// Label « Format ».
    FormatLabel,
    /// Label « Qualité ».
    QualityLabel,
    /// Préfixe « Destination : ».
    DestinationLabel,
    /// Titre « Aide de Photo ».
    HelpTitle,
    /// Aide : paragraphe outils.
    HelpToolsText,
    /// Aide : paragraphe calques.
    HelpLayersText,
    /// Type « Pixels » (inspecteur).
    LayerKindPixel,
    /// Type « Groupe ».
    LayerKindGroup,
    /// Type « Ajustement ».
    LayerKindAdjustment,
    /// Aide : paragraphe fichier/export.
    HelpFileText,
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

/// Toutes les clés (mettre à jour à chaque ajout : le test
/// `all_keys_translated` l'exige non vide dans les 2 langues).
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
    PhotoTextKey::MoveFilterUp,
    PhotoTextKey::MoveFilterDown,
    PhotoTextKey::DeleteFilter,
    PhotoTextKey::MoveMaskUp,
    PhotoTextKey::MoveMaskDown,
    PhotoTextKey::DeleteMask,
    PhotoTextKey::ShowHideLayer,
    PhotoTextKey::NoLayers,
    PhotoTextKey::NoLayerSelected,
    PhotoTextKey::FilterLiveLabel,
    PhotoTextKey::FiltersLabel,
    PhotoTextKey::MasksLabel,
    PhotoTextKey::MaskLabel,
    PhotoTextKey::FilterActive,
    PhotoTextKey::MaskActive,
    PhotoTextKey::VisibleLabel,
    PhotoTextKey::OpacityLabel,
    PhotoTextKey::BlendLabel,
    PhotoTextKey::FilterLabel,
    PhotoTextKey::FilterList,
    PhotoTextKey::AddFilterTitle,
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
        PhotoTextKey::Transform => "Transformation",
        PhotoTextKey::ExpandGroup => "Déplier",
        PhotoTextKey::CollapseGroup => "Replier",
        PhotoTextKey::ExpandAttachments => "Déplier les effets",
        PhotoTextKey::CollapseAttachments => "Replier les effets",
        PhotoTextKey::ToggleFilterEnabled => "Activer / désactiver le filtre",
        PhotoTextKey::ToggleMaskEnabled => "Activer / désactiver le masque",
        PhotoTextKey::MoveFilterUp => "Monter le filtre",
        PhotoTextKey::MoveFilterDown => "Descendre le filtre",
        PhotoTextKey::DeleteFilter => "Supprimer le filtre",
        PhotoTextKey::MoveMaskUp => "Monter le masque",
        PhotoTextKey::MoveMaskDown => "Descendre le masque",
        PhotoTextKey::DeleteMask => "Supprimer le masque",
        PhotoTextKey::ShowHideLayer => "Afficher / masquer",
        PhotoTextKey::NoLayers => "Aucun calque",
        PhotoTextKey::NoLayerSelected => "Aucun calque selectionne",
        PhotoTextKey::FilterLiveLabel => "Filtre live",
        PhotoTextKey::FiltersLabel => "Filtres",
        PhotoTextKey::MasksLabel => "Masques",
        PhotoTextKey::MaskLabel => "Masque",
        PhotoTextKey::FilterActive => "Filtre actif",
        PhotoTextKey::MaskActive => "Masque actif",
        PhotoTextKey::VisibleLabel => "Visible",
        PhotoTextKey::OpacityLabel => "Opacité",
        PhotoTextKey::BlendLabel => "Fusion",
        PhotoTextKey::FilterLabel => "Filtre",
        PhotoTextKey::FilterList => "Liste des filtres",
        PhotoTextKey::AddFilterTitle => "Ajouter un filtre",
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
        PhotoTextKey::Transform => "Transform",
        PhotoTextKey::ExpandGroup => "Expand",
        PhotoTextKey::CollapseGroup => "Collapse",
        PhotoTextKey::ExpandAttachments => "Expand effects",
        PhotoTextKey::CollapseAttachments => "Collapse effects",
        PhotoTextKey::ToggleFilterEnabled => "Enable / disable filter",
        PhotoTextKey::ToggleMaskEnabled => "Enable / disable mask",
        PhotoTextKey::MoveFilterUp => "Move filter up",
        PhotoTextKey::MoveFilterDown => "Move filter down",
        PhotoTextKey::DeleteFilter => "Delete filter",
        PhotoTextKey::MoveMaskUp => "Move mask up",
        PhotoTextKey::MoveMaskDown => "Move mask down",
        PhotoTextKey::DeleteMask => "Delete mask",
        PhotoTextKey::ShowHideLayer => "Show / hide",
        PhotoTextKey::NoLayers => "No layers",
        PhotoTextKey::NoLayerSelected => "No layer selected",
        PhotoTextKey::FilterLiveLabel => "Live filter",
        PhotoTextKey::FiltersLabel => "Filters",
        PhotoTextKey::MasksLabel => "Masks",
        PhotoTextKey::MaskLabel => "Mask",
        PhotoTextKey::FilterActive => "Filter enabled",
        PhotoTextKey::MaskActive => "Mask enabled",
        PhotoTextKey::VisibleLabel => "Visible",
        PhotoTextKey::OpacityLabel => "Opacity",
        PhotoTextKey::BlendLabel => "Blend",
        PhotoTextKey::FilterLabel => "Filter",
        PhotoTextKey::FilterList => "Filter list",
        PhotoTextKey::AddFilterTitle => "Add a filter",
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

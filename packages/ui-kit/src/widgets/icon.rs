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

//! Icônes sémantiques de Cygnus.
//!
//! SEUL fichier du workspace autorisé à importer
//! `egui_material_icons::icons`. Si on change de lib d'icônes un jour,
//! seul ce fichier est modifié : c'est la garantie anti-régression du
//! système d'icônes.
//!
//! RÈGLE ABSOLUE : aucun emoji, aucun glyphe unicode, aucun codepoint
//! en dur hors d'ici. Tout le code passe par [`CygnusIcon`].
//!
//! # Adaptation à `egui_material_icons` 0.8
//! Dans cette version, les constantes `ICON_*` sont des
//! [`egui_material_icons::MaterialIcon`] (codepoint + style), pas des
//! `&str`. [`CygnusIcon::glyph`] retourne donc directement la valeur de
//! la lib, et [`CygnusIcon::text`] utilise
//! [`egui_material_icons::icon_text`].

use egui_material_icons::icons::*;

/// Icônes sémantiques de Cygnus.
///
/// Chaque variante correspond à un usage métier (et non à un dessin
/// précis) : le mapping vers Material Symbols est encapsulé dans
/// [`CygnusIcon::glyph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CygnusIcon {
    /// Ajouter un élément.
    Add,
    /// Supprimer un élément.
    Remove,
    /// Calque visible.
    Visibility,
    /// Calque masqué.
    VisibilityOff,
    /// Panneau / liste des calques.
    Layers,
    /// Ajouter un calque.
    LayerAdd,
    /// Pinceau.
    Brush,
    /// Gomme.
    Eraser,
    /// Outil main (pan).
    Hand,
    /// Outil déplacement.
    MoveTool,
    /// Zoom avant.
    ZoomIn,
    /// Zoom arrière.
    ZoomOut,
    /// Annuler.
    Undo,
    /// Rétablir.
    Redo,
    /// Enregistrer.
    Save,
    /// Exporter / télécharger.
    Export,
    /// Paramètres.
    Settings,
    /// Fermer.
    Close,
    /// Poignée de drag.
    DragHandle,
    /// Ouvrir un dossier.
    FolderOpen,
    /// Calque image.
    ImageIcon,
    /// Masque / texture.
    Mask,
    /// Calque texte.
    Text,
    /// Calque forme vectorielle.
    Shape,
    /// Couper (video).
    Cut,
    /// Scinder un clip (video).
    Split,
    /// Rogner un clip (video, icône crop de la lib).
    Trim,
    /// Média film (video).
    Film,
    /// Lecture.
    Play,
    /// Pause.
    Pause,
    /// Stop.
    Stop,
    /// Enregistrement (audio).
    Record,
    /// Note de musique (audio).
    MusicNote,
    /// Piano (audio).
    Piano,
    /// Dupliquer (photo).
    Duplicate,
    /// Filtre (photo).
    Filter,
    /// Supprimer.
    Delete,
    /// Pipette.
    Eyedropper,
    /// Loupe / recherche (outil zoom).
    Search,
    /// Micro (audio).
    Mic,
    /// Succès / validation.
    Check,
    /// Avertissement.
    Warning,
    /// Erreur.
    Error,
    /// Information.
    Info,
    /// Déplier (chevron bas).
    ExpandMore,
    /// Replier (chevron haut).
    ExpandLess,
}

/// Liste exhaustive des icônes, utilisée par les tests.
pub const ALL_ICONS: &[CygnusIcon] = &[
    CygnusIcon::Add,
    CygnusIcon::Remove,
    CygnusIcon::Visibility,
    CygnusIcon::VisibilityOff,
    CygnusIcon::Layers,
    CygnusIcon::LayerAdd,
    CygnusIcon::Brush,
    CygnusIcon::Eraser,
    CygnusIcon::Hand,
    CygnusIcon::MoveTool,
    CygnusIcon::ZoomIn,
    CygnusIcon::ZoomOut,
    CygnusIcon::Undo,
    CygnusIcon::Redo,
    CygnusIcon::Save,
    CygnusIcon::Export,
    CygnusIcon::Settings,
    CygnusIcon::Close,
    CygnusIcon::DragHandle,
    CygnusIcon::FolderOpen,
    CygnusIcon::ImageIcon,
    CygnusIcon::Mask,
    CygnusIcon::Text,
    CygnusIcon::Shape,
    CygnusIcon::Cut,
    CygnusIcon::Split,
    CygnusIcon::Trim,
    CygnusIcon::Film,
    CygnusIcon::Play,
    CygnusIcon::Pause,
    CygnusIcon::Stop,
    CygnusIcon::Record,
    CygnusIcon::MusicNote,
    CygnusIcon::Piano,
    CygnusIcon::Mic,
    CygnusIcon::Duplicate,
    CygnusIcon::Filter,
    CygnusIcon::Delete,
    CygnusIcon::Eyedropper,
    CygnusIcon::Search,
    CygnusIcon::Check,
    CygnusIcon::Warning,
    CygnusIcon::Error,
    CygnusIcon::Info,
    CygnusIcon::ExpandMore,
    CygnusIcon::ExpandLess,
];

impl CygnusIcon {
    /// Icône brute de la lib (noms Material Symbols en SCREAMING_SNAKE_CASE).
    fn glyph(self) -> egui_material_icons::MaterialIcon {
        match self {
            Self::Add => ICON_ADD,
            Self::Remove => ICON_REMOVE,
            Self::Visibility => ICON_VISIBILITY,
            Self::VisibilityOff => ICON_VISIBILITY_OFF,
            Self::Layers => ICON_LAYERS,
            Self::LayerAdd => ICON_ADD_TO_PHOTOS,
            Self::Brush => ICON_BRUSH,
            Self::Eraser => ICON_INK_ERASER,
            Self::Hand => ICON_BACK_HAND,
            Self::MoveTool => ICON_OPEN_WITH,
            Self::ZoomIn => ICON_ZOOM_IN,
            Self::ZoomOut => ICON_ZOOM_OUT,
            Self::Undo => ICON_UNDO,
            Self::Redo => ICON_REDO,
            Self::Save => ICON_SAVE,
            Self::Export => ICON_DOWNLOAD,
            Self::Settings => ICON_SETTINGS,
            Self::Close => ICON_CLOSE,
            Self::DragHandle => ICON_DRAG_INDICATOR,
            Self::FolderOpen => ICON_FOLDER_OPEN,
            Self::ImageIcon => ICON_IMAGE,
            Self::Mask => ICON_TEXTURE,
            Self::Text => ICON_TITLE,
            Self::Shape => ICON_SHAPES,
            Self::Cut => ICON_CONTENT_CUT,
            Self::Split => ICON_CALL_SPLIT,
            Self::Trim => ICON_CROP,
            Self::Film => ICON_MOVIE,
            Self::Play => ICON_PLAY_ARROW,
            Self::Pause => ICON_PAUSE,
            Self::Stop => ICON_STOP,
            Self::Record => ICON_FIBER_MANUAL_RECORD,
            Self::MusicNote => ICON_MUSIC_NOTE,
            Self::Piano => ICON_PIANO,
            Self::Mic => ICON_MIC,
            Self::Duplicate => ICON_CONTENT_COPY,
            Self::Filter => ICON_FILTER_LIST,
            Self::Delete => ICON_DELETE,
            Self::Eyedropper => ICON_COLORIZE,
            Self::Search => ICON_SEARCH,
            Self::Check => ICON_CHECK,
            Self::Warning => ICON_WARNING,
            Self::Error => ICON_ERROR,
            Self::Info => ICON_INFO,
            Self::ExpandMore => ICON_EXPAND_MORE,
            Self::ExpandLess => ICON_EXPAND_LESS,
        }
    }

    /// Codepoint brut (pour les tests anti-tofu uniquement).
    #[cfg(test)]
    pub(crate) fn codepoint(self) -> &'static str {
        self.glyph().codepoint
    }

    /// RichText prêt à afficher (taille par défaut du thème).
    pub fn text(self) -> egui::RichText {
        egui_material_icons::icon_text(self.glyph())
    }

    /// RichText à la taille demandée.
    pub fn sized(self, size: f32) -> egui::RichText {
        self.text().size(size)
    }

    /// RichText teinté de la couleur demandée.
    pub fn colored(self, color: egui::Color32) -> egui::RichText {
        self.text().color(color)
    }
}

/// Bouton icône standard Cygnus : sans frame, fond au hover, tooltip optionnel.
///
/// Utilisé PARTOUT (toolbar, panels, layers) pour garantir l'uniformité
/// entre les 3 apps.
///
/// # Exemple
/// ```rust,no_run
/// # use ui_kit::widgets::icon::{CygnusIcon, icon_button};
/// # egui::__run_test_ui(|ui| {
/// let response = icon_button(ui, CygnusIcon::Save, Some("Enregistrer"));
/// if response.clicked() {
///     // … déclencher la sauvegarde via un channel moteur …
/// }
/// # });
/// ```
pub fn icon_button(ui: &mut egui::Ui, icon: CygnusIcon, tooltip: Option<&str>) -> egui::Response {
    let response = ui.add(egui::Button::new(icon.sized(18.0)).frame(false));
    if let Some(tip) = tooltip {
        response.clone().on_hover_text(tip)
    } else {
        response
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::setup_fonts;

    #[test]
    fn all_icons_have_glyph() {
        for icon in ALL_ICONS {
            assert!(!icon.codepoint().is_empty(), "glyphe vide pour {icon:?}");
        }
    }

    #[test]
    fn all_icons_list_is_exhaustive() {
        // 46 variantes déclarées : le test casse si une variante est
        // ajoutée sans être enregistrée dans ALL_ICONS.
        assert_eq!(ALL_ICONS.len(), 46);
    }

    #[test]
    fn icons_render_without_panic() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for icon in ALL_ICONS {
                    ui.label(icon.text());
                }
            });
        });
        assert!(
            !output.shapes.is_empty(),
            "aucune primitive de rendu produite"
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn icon_button_renders_without_panic() {
        let ctx = egui::Context::default();
        setup_fonts(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                for icon in ALL_ICONS {
                    let _ = icon_button(ui, *icon, Some("tip"));
                }
            });
        })
        .drop_without_applying_deltas();
    }
}

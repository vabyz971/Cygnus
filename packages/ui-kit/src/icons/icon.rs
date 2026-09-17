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

//! Icônes sémantiques du design system.
//!
//! [`Icon`] est un enum stable et indépendant du domaine : les apps
//! Photo, Video et Audio l'utilisent sans jamais importer une
//! bibliothèque d'icônes. La résolution vers le dessin réel passe par
//! [`IconRegistry`], qui délègue à
//! [`CygnusIcon`](crate::widgets::icon::CygnusIcon) — SEUL contact du
//! workspace avec `egui_material_icons`.

/// Icône sémantique (usage métier, pas un dessin précis).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Enregistrer.
    Save,
    /// Ouvrir.
    Open,
    /// Annuler.
    Undo,
    /// Rétablir.
    Redo,
    /// Fermer.
    Close,
    /// Zoom avant.
    ZoomIn,
    /// Zoom arrière.
    ZoomOut,
    /// Supprimer.
    Delete,
    /// Ajouter.
    Add,
    /// Dupliquer.
    Duplicate,
    /// Exporter.
    Export,
    /// Paramètres.
    Settings,
    /// Lecture.
    Play,
    /// Pause.
    Pause,
    /// Recherche / loupe.
    Search,
    /// Validation / succès.
    Check,
    /// Information.
    Info,
    /// Dossier ouvert.
    Folder,
    /// Calque visible.
    Visibility,
    /// Calque masqué.
    VisibilityOff,
    /// Panneau / liste des calques.
    Layers,
    /// Icône image (calque pixel).
    ImageIcon,
    /// Pinceau (outil peinture).
    Brush,
    /// Gomme (outil).
    Eraser,
    /// Main (outil pan).
    Hand,
    /// Déplacement / sélection.
    MoveTool,
    /// Déplier (chevron bas).
    ExpandMore,
    /// Replier (chevron haut).
    ExpandLess,
    /// Pipette (détection couleur).
    Eyedropper,
    /// Filtre (liste filtres).
    Filter,
    /// Masque / texture.
    Mask,
    /// Poignée de glisser-déposer.
    DragHandle,
    /// Texte (calque).
    Text,
    /// Forme vectorielle.
    Shape,
    /// Couper.
    Cut,
    /// Diverger (video).
    Split,
    /// Rougeigner (video).
    Trim,
    /// Film (vidéo).
    Film,
    /// Arrêt.
    Stop,
    /// Enregistrement (audio).
    Record,
    /// Note musicale (audio).
    MusicNote,
    /// Piano (audio).
    Piano,
    /// Micro (audio).
    Mic,
    /// Avertissement.
    Warning,
    /// Erreur.
    Error,
    /// Supprimer un élément.
    Remove,
    /// Ajouter un calque.
    LayerAdd,
}

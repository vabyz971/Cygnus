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

//! Clés de traduction stables du design system.
//!
//! Les composants ne portent aucune chaîne métier : les apps
//! traduisent via [`Catalog`](super::catalog::Catalog). Les clés sont
//! stables entre versions (sérialisables, jamais renommées).

use serde::{Deserialize, Serialize};

/// Clé de texte stable (partagée Photo / Video / Audio).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextKey {
    /// Enregistrer.
    Save,
    /// Ouvrir.
    Open,
    /// Annuler l'action.
    Cancel,
    /// Fermer.
    Close,
    /// Annuler (undo).
    Undo,
    /// Rétablir (redo).
    Redo,
    /// Supprimer.
    Delete,
    /// Dupliquer.
    Duplicate,
    /// Nouveau document.
    NewDocument,
    /// Exporter.
    Export,
    /// Calques.
    Layers,
    /// Paramètres.
    Settings,
    /// Quitter.
    Quit,
    /// Copier.
    Copy,
    /// Coller.
    Paste,
    /// Outils.
    Tools,
    /// Inspecteur de propriétés.
    Inspector,
    /// Navigateur.
    Navigator,
    /// Historique.
    History,
    /// Chronologie.
    Timeline,
}

impl TextKey {
    /// Libellé anglais par défaut (langue de repli).
    pub fn default_text(self) -> &'static str {
        match self {
            Self::Save => "Save",
            Self::Open => "Open",
            Self::Cancel => "Cancel",
            Self::Close => "Close",
            Self::Undo => "Undo",
            Self::Redo => "Redo",
            Self::Delete => "Delete",
            Self::Duplicate => "Duplicate",
            Self::NewDocument => "New document",
            Self::Export => "Export",
            Self::Layers => "Layers",
            Self::Settings => "Settings",
            Self::Quit => "Quit",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::Tools => "Tools",
            Self::Inspector => "Inspector",
            Self::Navigator => "Navigator",
            Self::History => "History",
            Self::Timeline => "Timeline",
        }
    }
}

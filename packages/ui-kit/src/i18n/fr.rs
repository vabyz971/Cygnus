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

//! Table française.
//!
//! Une entrée par [`TextKey`](super::key::TextKey), dans l'ordre de
//! l'enum : le `match` exhaustif casse à l'ajout d'une clé tant
//! qu'elle n'est pas traduite ici (et en anglais, voir
//! [`super::en`]).

use super::key::TextKey;

/// Traduit `key` en français.
pub fn translate(key: TextKey) -> &'static str {
    match key {
        TextKey::Save => "Enregistrer",
        TextKey::Open => "Ouvrir",
        TextKey::Cancel => "Annuler",
        TextKey::Close => "Fermer",
        TextKey::Undo => "Annuler",
        TextKey::Redo => "Rétablir",
        TextKey::Delete => "Supprimer",
        TextKey::Duplicate => "Dupliquer",
        TextKey::NewDocument => "Nouveau document",
        TextKey::Export => "Exporter",
        TextKey::Layers => "Calques",
        TextKey::Settings => "Paramètres",
        TextKey::Quit => "Quitter",
        TextKey::Copy => "Copier",
        TextKey::Paste => "Coller",
        TextKey::Tools => "Outils",
        TextKey::Inspector => "Inspecteur",
        TextKey::Navigator => "Navigateur",
        TextKey::History => "Historique",
        TextKey::Timeline => "Chronologie",
        TextKey::File => "Fichier",
        TextKey::Edit => "Édition",
        TextKey::View => "Affichage",
        TextKey::Help => "Aide",
        TextKey::ZoomIn => "Zoom avant",
        TextKey::ZoomOut => "Zoom arrière",
        TextKey::Grid => "Grille",
        TextKey::Canvas => "Canevas",
        TextKey::Window => "Fenêtre",
    }
}

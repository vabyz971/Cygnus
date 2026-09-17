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

//! Table anglaise (langue de repli).
//!
//! Une entrée par [`TextKey`](super::key::TextKey), dans l'ordre de
//! l'enum : le `match` exhaustif casse à l'ajout d'une clé tant
//! qu'elle n'est pas traduite ici (et en français, voir
//! [`super::fr`]).

use super::key::TextKey;

/// Traduit `key` en anglais.
pub fn translate(key: TextKey) -> &'static str {
    match key {
        TextKey::Save => "Save",
        TextKey::Open => "Open",
        TextKey::Cancel => "Cancel",
        TextKey::Close => "Close",
        TextKey::Undo => "Undo",
        TextKey::Redo => "Redo",
        TextKey::Delete => "Delete",
        TextKey::Duplicate => "Duplicate",
        TextKey::NewDocument => "New document",
        TextKey::Export => "Export",
        TextKey::Layers => "Layers",
        TextKey::Settings => "Settings",
        TextKey::Quit => "Quit",
        TextKey::Copy => "Copy",
        TextKey::Paste => "Paste",
        TextKey::Tools => "Tools",
        TextKey::Inspector => "Inspector",
        TextKey::Navigator => "Navigator",
        TextKey::History => "History",
        TextKey::Timeline => "Timeline",
        TextKey::File => "File",
        TextKey::Edit => "Edit",
        TextKey::View => "View",
        TextKey::Help => "Help",
        TextKey::ZoomIn => "Zoom in",
        TextKey::ZoomOut => "Zoom out",
        TextKey::Grid => "Grid",
        TextKey::Canvas => "Canvas",
        TextKey::Window => "Window",
    }
}

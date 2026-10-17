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

//! Stable translation keys of the design system.
//!
//! Components carry no domain string: apps translate via
//! [`Catalog`](super::catalog::Catalog), whose tables live in
//! [`super::en`] (English) and [`super::fr`] (French). Keys are
//! stable across versions (serializable, never renamed).

use serde::{Deserialize, Serialize};

/// Stable text key (shared Photo / Video / Audio).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TextKey {
    /// Save.
    Save,
    /// Open.
    Open,
    /// Cancel action.
    Cancel,
    /// Close.
    Close,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Delete.
    Delete,
    /// Rename.
    Rename,
    /// Show.
    Show,
    /// Hide.
    Hide,
    /// Add.
    Add,
    /// Create.
    Create,
    /// Confirm.
    Validate,
    /// Duplicate.
    Duplicate,
    /// New document.
    NewDocument,
    /// Export.
    Export,
    /// Layer.
    Layer,
    /// Layers.
    Layers,
    /// Settings.
    Settings,
    /// Quit.
    Quit,
    /// Copy.
    Copy,
    /// Paste.
    Paste,
    /// Tools.
    Tools,
    /// Property inspector.
    Inspector,
    /// Navigator.
    Navigator,
    /// History.
    History,
    /// Histogram (tone distribution).
    Histogram,
    /// Timeline.
    Timeline,
    /// File menu.
    File,
    /// Edit menu.
    Edit,
    /// View menu.
    View,
    /// Help menu.
    Help,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Grid.
    Grid,
    /// Canvas (drawing area).
    Canvas,
    /// Window (dockable-panel menu).
    Window,
}

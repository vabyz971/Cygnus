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

//! Design-system semantic icons.
//!
//! [`Icon`] is a stable, domain-independent enum: the Photo, Video
//! and Audio apps use it without ever importing an icon library.
//! Resolution to the actual drawing goes through
//! [`IconRegistry`](super::registry::IconRegistry), which delegates
//! to the internal [`glyph`](super::glyph) module — the workspace's
//! ONLY contact with `egui_material_icons::icons`.

/// Semantic icon (domain usage, not a specific drawing).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Save.
    Save,
    /// Open.
    Open,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Close.
    Close,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Delete.
    Delete,
    /// Add.
    Add,
    /// Duplicate.
    Duplicate,
    /// Export.
    Export,
    /// Settings.
    Settings,
    /// Play.
    Play,
    /// Pause.
    Pause,
    /// Search / magnifier.
    Search,
    /// Validation / success.
    Check,
    /// Information.
    Info,
    /// Open folder.
    Folder,
    /// Visible layer.
    Visibility,
    /// Hidden layer.
    VisibilityOff,
    /// Layers panel / list.
    Layers,
    /// Image icon (pixel layer).
    ImageIcon,
    /// Brush (paint tool).
    Brush,
    /// Eraser (tool).
    Eraser,
    /// Hand (pan tool).
    Hand,
    /// Move / select.
    MoveTool,
    /// Expand (chevron down).
    ExpandMore,
    /// Collapse (chevron up).
    ExpandLess,
    /// Eyedropper (color picking).
    Eyedropper,
    /// Filter (filter list).
    Filter,
    /// Mask / texture.
    Mask,
    /// Drag-and-drop handle.
    DragHandle,
    /// Text (layer).
    Text,
    /// Vector shape.
    Shape,
    /// Node editing (vector pen nodes).
    Node,
    /// Cut.
    Cut,
    /// Split (video).
    Split,
    /// Trim (video).
    Trim,
    /// Film (video).
    Film,
    /// Stop.
    Stop,
    /// Record (audio).
    Record,
    /// Music note (audio).
    MusicNote,
    /// Piano (audio).
    Piano,
    /// Mic (audio).
    Mic,
    /// Warning.
    Warning,
    /// Error.
    Error,
    /// Remove an item.
    Remove,
    /// Add a layer.
    LayerAdd,
    /// Overflow menu (three horizontal dots).
    More,
    /// Minimize the window.
    Minimize,
    /// Maximize / restore the window.
    Maximize,
    /// Raster (pixel) persona.
    Camera,
    /// Vector persona / pen.
    Pen,
    /// Layout persona (stacked pages).
    Stacks,
    /// Side-by-side view.
    SplitView,
    /// Panel layout (dock).
    Workspace,
    /// Locked item.
    Lock,
    /// Adjustment settings.
    Tune,
    /// Tone curve.
    Curve,
    /// Step history.
    History,
    /// Disclosure / next.
    ChevronRight,
    /// Swap two values (colors).
    Swap,
    /// Tone adjustment.
    Contrast,
    /// Grid overlay.
    Grid,
    /// Rectangle selection (marquee).
    Marquee,
    /// Ellipse outline (elliptical marquee).
    EllipseOutline,
    /// Freehand (lasso) selection.
    Lasso,
    /// Magic wand selection.
    Wand,
}

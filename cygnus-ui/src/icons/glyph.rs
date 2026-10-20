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

//! Internal mapping between semantic icons and Material Symbols.
//!
//! ONLY workspace file allowed to import `egui_material_icons::icons`.
//! Switching icon libs one day touches this file alone: the icon
//! system's anti-regression guarantee.
//!
//! ABSOLUTE RULE: no emoji, no unicode glyph, no hardcoded
//! codepoint outside this file. All code goes through [`CygnusIcon`],
//! exposed publicly as [`Icon`](super::icon::Icon) via the registry.

use egui_material_icons::icons::*;

/// Internal icon (Material Symbols resolution).
///
/// Each variant maps to a domain usage (not a specific drawing):
/// the mapping to Material Symbols is encapsulated in
/// [`CygnusIcon::glyph`]. Apps use [`Icon`](super::icon::Icon);
/// this type stays `pub(crate)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CygnusIcon {
    /// Add an item.
    Add,
    /// Remove an item.
    Remove,
    /// Visible layer.
    Visibility,
    /// Hidden layer.
    VisibilityOff,
    /// Layers panel / list.
    Layers,
    /// Add a layer.
    LayerAdd,
    /// Brush.
    Brush,
    /// Eraser.
    Eraser,
    /// Hand tool (pan).
    Hand,
    /// Move tool.
    MoveTool,
    /// Zoom in.
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Undo.
    Undo,
    /// Redo.
    Redo,
    /// Save.
    Save,
    /// Export / download.
    Export,
    /// Settings.
    Settings,
    /// Close.
    Close,
    /// Drag handle.
    DragHandle,
    /// Open a folder.
    FolderOpen,
    /// Image layer.
    ImageIcon,
    /// Mask / texture.
    Mask,
    /// Text layer.
    Text,
    /// Vector-shape layer.
    Shape,
    /// Node editing (vector pen nodes).
    Node,
    /// Cut (video).
    Cut,
    /// Split a clip (video).
    Split,
    /// Trim a clip (video, lib crop icon).
    Trim,
    /// Film media (video).
    Film,
    /// Play.
    Play,
    /// Pause.
    Pause,
    /// Stop.
    Stop,
    /// Record (audio).
    Record,
    /// Music note (audio).
    MusicNote,
    /// Piano (audio).
    Piano,
    /// Duplicate (photo).
    Duplicate,
    /// Filter (photo).
    Filter,
    /// Delete.
    Delete,
    /// Eyedropper.
    Eyedropper,
    /// Magnifier / search (zoom tool).
    Search,
    /// Mic (audio).
    Mic,
    /// Success / validation.
    Check,
    /// Warning.
    Warning,
    /// Error.
    Error,
    /// Information.
    Info,
    /// Expand (chevron down).
    ExpandMore,
    /// Collapse (chevron up).
    ExpandLess,
    More,
    Minimize,
    Maximize,
    Camera,
    Pen,
    Stacks,
    SplitView,
    Workspace,
    Lock,
    Tune,
    Curve,
    History,
    ChevronRight,
    Swap,
    Contrast,
    Grid,
    Marquee,
    EllipseOutline,
    Lasso,
    Wand,
    PaintBucket,
    Gradient,
    Stamp,
}

impl CygnusIcon {
    /// Raw lib icon (Material Symbols names in SCREAMING_SNAKE_CASE).
    pub(crate) fn glyph(self) -> egui_material_icons::MaterialIcon {
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
            Self::Export => ICON_UPLOAD,
            Self::Settings => ICON_SETTINGS,
            Self::Close => ICON_CLOSE,
            Self::DragHandle => ICON_DRAG_INDICATOR,
            Self::FolderOpen => ICON_FOLDER_OPEN,
            Self::ImageIcon => ICON_IMAGE,
            Self::Mask => ICON_TEXTURE,
            Self::Text => ICON_TITLE,
            Self::Shape => ICON_SHAPES,
            Self::Node => ICON_POLYLINE,
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
            Self::More => ICON_MORE_HORIZ,
            Self::Minimize => ICON_MINIMIZE,
            Self::Maximize => ICON_CROP_SQUARE,
            Self::Camera => ICON_CAMERA,
            Self::Pen => ICON_DRAW,
            Self::Stacks => ICON_STACKS,
            Self::SplitView => ICON_VERTICAL_SPLIT,
            Self::Workspace => ICON_SPACE_DASHBOARD,
            Self::Lock => ICON_LOCK,
            Self::Tune => ICON_TUNE,
            Self::Curve => ICON_LINE_CURVE,
            Self::History => ICON_HISTORY,
            Self::ChevronRight => ICON_CHEVRON_RIGHT,
            Self::Swap => ICON_SWAP_HORIZ,
            Self::Contrast => ICON_CONTRAST,
            Self::Grid => ICON_GRID_4X4,
            Self::Marquee => ICON_SELECT,
            Self::EllipseOutline => ICON_RADIO_BUTTON_UNCHECKED,
            Self::Lasso => ICON_LASSO_SELECT,
            Self::Wand => ICON_AUTO_FIX_HIGH,
            Self::PaintBucket => ICON_FORMAT_COLOR_FILL,
            Self::Gradient => ICON_GRADIENT,
            Self::Stamp => ICON_APPROVAL,
        }
    }

    /// Raw codepoint (for anti-tofu tests only).
    #[cfg(test)]
    pub(crate) fn codepoint(self) -> &'static str {
        self.glyph().codepoint
    }

    /// Display-ready RichText (theme default size).
    pub(crate) fn text(self) -> egui::RichText {
        egui_material_icons::icon_text(self.glyph())
    }

    /// RichText at the requested size.
    pub(crate) fn sized(self, size: f32) -> egui::RichText {
        self.text().size(size)
    }
}

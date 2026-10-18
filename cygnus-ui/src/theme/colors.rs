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

//! Cygnus theme colors.
//!
//! SINGLE source of the suite's colors: no component, panel
//! or canvas may hardcode an [`egui::Color32`]. The dark-theme
//! values live in [`CygnusColors::dark`], reused by
//! [`Default`] for direct access.
//!
//! "Atelier" palette: cool blue-grey surfaces separated by 1 px
//! dividers, three text levels (strong / default / subdued) and one
//! soft lavender accent. Chrome (`bg_primary`: bars, tool rail) sits
//! between the recessed areas (`bg_canvas` around the document,
//! `bg_tertiary` for inputs and plots) and the raised side panels
//! (`bg_secondary`, also popups); `widget_bg` fills resting controls.

/// Cygnus theme colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CygnusColors {
    /// Main panel background.
    pub bg_primary: egui::Color32,
    /// Window and bar background.
    pub bg_secondary: egui::Color32,
    /// Recessed-area background (inputs, plots).
    pub bg_tertiary: egui::Color32,
    /// Deepest surface: the area around the document on the canvas.
    pub bg_canvas: egui::Color32,
    /// Primary (default) text.
    pub fg_primary: egui::Color32,
    /// Secondary text (labels, captions).
    pub fg_secondary: egui::Color32,
    /// Strong text: headings, active/hovered items, selected tabs.
    pub fg_strong: egui::Color32,
    /// Subdued text: placeholders, disabled hints.
    pub fg_subdued: egui::Color32,
    /// Fill of resting controls (slider rails, checkboxes, toggles).
    pub widget_bg: egui::Color32,
    /// Shadow under popups, menus and windows.
    pub shadow: egui::Color32,
    /// Accent (selection, primary buttons).
    pub accent: egui::Color32,
    /// Text and icons drawn on an `accent` fill.
    pub fg_on_accent: egui::Color32,
    /// Hover accent.
    pub accent_hover: egui::Color32,
    /// Borders.
    pub border: egui::Color32,
    /// Error (also used for destructive actions).
    pub error: egui::Color32,
    /// Hover error (destructive buttons).
    pub error_hover: egui::Color32,
    /// Success.
    pub success: egui::Color32,
    /// Warning.
    pub warning: egui::Color32,
    /// Selected-item background (layer, clip, track).
    pub item_selected: egui::Color32,
    /// Hovered-item background.
    pub item_hover: egui::Color32,
    /// Drop-position indicator (drag & drop).
    pub drop_indicator: egui::Color32,
    /// Red channel plot (histograms, per-channel graphs).
    pub channel_red: egui::Color32,
    /// Green channel plot.
    pub channel_green: egui::Color32,
    /// Blue channel plot.
    pub channel_blue: egui::Color32,
}

impl CygnusColors {
    /// Default Cygnus dark palette.
    pub fn dark() -> Self {
        Self {
            bg_primary: egui::Color32::from_rgb(27, 30, 37),
            bg_secondary: egui::Color32::from_rgb(32, 36, 45),
            bg_tertiary: egui::Color32::from_rgb(23, 27, 33),
            bg_canvas: egui::Color32::from_rgb(20, 23, 28),
            fg_primary: egui::Color32::from_rgb(206, 209, 216),
            fg_secondary: egui::Color32::from_rgb(140, 146, 160),
            fg_strong: egui::Color32::from_rgb(236, 238, 244),
            fg_subdued: egui::Color32::from_rgb(95, 101, 114),
            widget_bg: egui::Color32::from_rgb(44, 48, 58),
            shadow: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 140),
            accent: egui::Color32::from_rgb(170, 172, 242),
            fg_on_accent: egui::Color32::from_rgb(23, 25, 38),
            accent_hover: egui::Color32::from_rgb(194, 197, 252),
            border: egui::Color32::from_rgb(44, 48, 58),
            error: egui::Color32::from_rgb(229, 92, 102),
            error_hover: egui::Color32::from_rgb(240, 116, 124),
            success: egui::Color32::from_rgb(110, 196, 150),
            warning: egui::Color32::from_rgb(236, 166, 92),
            item_selected: egui::Color32::from_rgb(54, 56, 79),
            item_hover: egui::Color32::from_rgb(40, 44, 55),
            drop_indicator: egui::Color32::from_rgb(170, 172, 242),
            channel_red: egui::Color32::from_rgb(196, 98, 128),
            channel_green: egui::Color32::from_rgb(98, 158, 136),
            channel_blue: egui::Color32::from_rgb(92, 126, 170),
        }
    }
}

/// Displays a user/document color (brush swatch, picked sample).
///
/// The only sanctioned way to turn DATA into a `Color32` outside this
/// module: such colors belong to the document, never to the interface,
/// so they bypass the palette on purpose.
#[must_use]
pub fn user_color(rgb: [u8; 3]) -> egui::Color32 {
    egui::Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

impl Default for CygnusColors {
    /// Default dark palette.
    fn default() -> Self {
        Self::dark()
    }
}

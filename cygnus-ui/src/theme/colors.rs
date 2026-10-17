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
//! Palette modeled on Rerun's viewer (`re_ui` design tokens): near-black
//! neutral surfaces separated by 1 px dividers rather than by contrasting
//! fills, three text levels (strong / default / subdued) and one blue
//! accent. Surfaces step up in lightness with elevation:
//! `bg_primary` (panels) < `bg_secondary` (bars, popups) <
//! `bg_tertiary` (inputs, canvas surround) < `widget_bg` (controls).

/// Cygnus theme colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CygnusColors {
    /// Main panel background.
    pub bg_primary: egui::Color32,
    /// Window and bar background.
    pub bg_secondary: egui::Color32,
    /// Recessed-area background (inputs, empty canvas).
    pub bg_tertiary: egui::Color32,
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
            bg_primary: egui::Color32::from_rgb(13, 13, 13),
            bg_secondary: egui::Color32::from_rgb(23, 23, 23),
            bg_tertiary: egui::Color32::from_rgb(33, 33, 33),
            fg_primary: egui::Color32::from_rgb(204, 204, 204),
            fg_secondary: egui::Color32::from_rgb(147, 144, 144),
            fg_strong: egui::Color32::from_rgb(255, 255, 255),
            fg_subdued: egui::Color32::from_rgb(100, 99, 99),
            widget_bg: egui::Color32::from_rgb(44, 43, 43),
            shadow: egui::Color32::from_rgba_unmultiplied(0, 0, 0, 128),
            accent: egui::Color32::from_rgb(24, 106, 221),
            accent_hover: egui::Color32::from_rgb(70, 118, 255),
            border: egui::Color32::from_rgb(53, 52, 52),
            error: egui::Color32::from_rgb(229, 72, 77),
            error_hover: egui::Color32::from_rgb(240, 98, 102),
            success: egui::Color32::from_rgb(70, 190, 120),
            warning: egui::Color32::from_rgb(255, 122, 12),
            item_selected: egui::Color32::from_rgb(0, 61, 161),
            item_hover: egui::Color32::from_rgb(39, 38, 38),
            drop_indicator: egui::Color32::from_rgb(24, 106, 221),
            channel_red: egui::Color32::from_rgb(235, 80, 80),
            channel_green: egui::Color32::from_rgb(80, 200, 110),
            channel_blue: egui::Color32::from_rgb(85, 130, 245),
        }
    }
}

impl Default for CygnusColors {
    /// Default dark palette.
    fn default() -> Self {
        Self::dark()
    }
}

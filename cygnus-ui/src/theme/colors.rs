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

/// Cygnus theme colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CygnusColors {
    /// Main panel background.
    pub bg_primary: egui::Color32,
    /// Window and bar background.
    pub bg_secondary: egui::Color32,
    /// Recessed-area background (inputs, empty canvas).
    pub bg_tertiary: egui::Color32,
    /// Primary text.
    pub fg_primary: egui::Color32,
    /// Secondary / disabled text.
    pub fg_secondary: egui::Color32,
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
}

impl CygnusColors {
    /// Default Cygnus dark palette.
    pub fn dark() -> Self {
        Self {
            bg_primary: egui::Color32::from_rgb(18, 18, 22),
            bg_secondary: egui::Color32::from_rgb(24, 24, 30),
            bg_tertiary: egui::Color32::from_rgb(32, 32, 40),
            fg_primary: egui::Color32::from_rgb(240, 240, 245),
            fg_secondary: egui::Color32::from_rgb(160, 160, 175),
            accent: egui::Color32::from_rgb(88, 124, 255),
            accent_hover: egui::Color32::from_rgb(108, 144, 255),
            border: egui::Color32::from_rgb(48, 48, 60),
            error: egui::Color32::from_rgb(255, 85, 85),
            error_hover: egui::Color32::from_rgb(255, 105, 105),
            success: egui::Color32::from_rgb(80, 220, 120),
            warning: egui::Color32::from_rgb(255, 190, 60),
            item_selected: egui::Color32::from_rgb(45, 55, 90),
            item_hover: egui::Color32::from_rgb(35, 35, 45),
            drop_indicator: egui::Color32::from_rgb(88, 124, 255),
        }
    }
}

impl Default for CygnusColors {
    /// Default dark palette.
    fn default() -> Self {
        Self::dark()
    }
}

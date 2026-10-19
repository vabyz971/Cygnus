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

//! Cygnus theme corner radii.
//!
//! SINGLE source of radii: the `corner_radius` of `egui::Frame`
//! and custom buttons reference these tokens.

/// Theme corner radii.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusRadius {
    /// Small radius (2px).
    pub xs: f32,
    /// No rounding.
    pub none: f32,
    /// Small radius (4px).
    pub sm: f32,
    /// Medium radius (8px).
    pub md: f32,
    /// Large radius (12px).
    pub lg: f32,
    /// "Pill" radius (circles).
    pub full: f32,
}

impl CygnusRadius {
    /// Dark-theme radii.
    pub fn dark() -> Self {
        Self {
            none: 0.0,
            xs: 2.0,
            sm: 4.0,
            md: 6.0,
            lg: 10.0,
            full: 9999.0,
        }
    }
}

impl Default for CygnusRadius {
    /// Default radii.
    fn default() -> Self {
        Self::dark()
    }
}

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

//! Cygnus theme borders.
//!
//! SINGLE source of stroke widths: the `egui::Stroke` of
//! cards, surfaces and focus reference these tokens. Border
//! color stays in [`super::colors`] (`border`).

/// Border widths.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusBorders {
    /// Standard thin stroke (1px).
    pub thin: f32,
    /// Medium stroke, emphasis (2px).
    pub medium: f32,
    /// Thick stroke, drag & drop (3px).
    pub thick: f32,
    /// Selection stroke (2.5px).
    pub selected: f32,
}

impl CygnusBorders {
    /// Dark-theme borders.
    pub fn dark() -> Self {
        Self {
            thin: 1.0,
            medium: 2.0,
            thick: 3.0,
            selected: 2.5,
        }
    }
}

impl Default for CygnusBorders {
    /// Default borders.
    fn default() -> Self {
        Self::dark()
    }
}

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

//! Cygnus theme spacing.
//!
//! SINGLE source of spacing: component margins, paddings and gutters
//! reference these tokens instead of `f32` literals.

/// Theme spacing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusSpacing {
    /// Extra-extra-small spacing (2px).
    pub xxs: f32,
    /// Extra-small spacing (4px).
    pub xs: f32,
    /// Small spacing (8px).
    pub sm: f32,
    /// Medium spacing (12px).
    pub md: f32,
    /// Large spacing (16px).
    pub lg: f32,
    /// Extra-large spacing (24px).
    pub xl: f32,
}

impl CygnusSpacing {
    /// Dark-theme spacing scale.
    pub fn dark() -> Self {
        Self {
            xxs: 2.0,
            xs: 4.0,
            sm: 8.0,
            md: 12.0,
            lg: 16.0,
            xl: 24.0,
        }
    }
}

impl Default for CygnusSpacing {
    /// Default spacing scale.
    fn default() -> Self {
        Self::dark()
    }
}

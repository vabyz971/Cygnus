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

//! Espacements du thème Cygnus.
//!
//! SEULE source des espacements : marges, paddings et gouttières des
//! composants référencent ces tokens au lieu de littéraux `f32`.

/// Espacements du thème.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusSpacing {
    /// Très petit espacement (4px).
    pub xs: f32,
    /// Petit espacement (8px).
    pub sm: f32,
    /// Espacement moyen (12px).
    pub md: f32,
    /// Grand espacement (16px).
    pub lg: f32,
    /// Très grand espacement (24px).
    pub xl: f32,
}

impl CygnusSpacing {
    /// Échelle d'espacements du thème sombre.
    pub fn dark() -> Self {
        Self {
            xs: 4.0,
            sm: 8.0,
            md: 12.0,
            lg: 16.0,
            xl: 24.0,
        }
    }
}

impl Default for CygnusSpacing {
    /// Échelle d'espacements par défaut.
    fn default() -> Self {
        Self::dark()
    }
}

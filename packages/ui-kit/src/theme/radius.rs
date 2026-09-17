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

//! Rayons de coins du thème Cygnus.
//!
//! SEULE source des rayons : les `corner_radius` des `egui::Frame`
//! et boutons custom référencent ces tokens.

/// Rayons de coins du thème.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusRadius {
    /// Petit rayon (2px).
    pub xs: f32,
    /// Pas d'arrondi.
    pub none: f32,
    /// Petit rayon (4px).
    pub sm: f32,
    /// Rayon moyen (8px).
    pub md: f32,
    /// Grand rayon (12px).
    pub lg: f32,
    /// Rayon "pilule" (cercles).
    pub full: f32,
}

impl CygnusRadius {
    /// Rayons du thème sombre.
    pub fn dark() -> Self {
        Self {
            none: 0.0,
            xs: 2.0,
            sm: 4.0,
            md: 8.0,
            lg: 12.0,
            full: 9999.0,
        }
    }
}

impl Default for CygnusRadius {
    /// Rayons par défaut.
    fn default() -> Self {
        Self::dark()
    }
}

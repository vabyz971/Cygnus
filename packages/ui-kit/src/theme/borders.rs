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

//! Bordures du thème Cygnus.
//!
//! SEULE source des épaisseurs de trait : les `egui::Stroke` des
//! cartes, surfaces et focus référencent ces tokens. La couleur des
//! bordures reste dans [`super::colors`] (`border`).

/// Épaisseurs de bordure.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusBorders {
    /// Trait fin standard (1px).
    pub thin: f32,
    /// Trait moyen, emphase (2px).
    pub medium: f32,
    /// Trait épais, drag & drop (3px).
    pub thick: f32,
}

impl CygnusBorders {
    /// Bordures du thème sombre.
    pub fn dark() -> Self {
        Self {
            thin: 1.0,
            medium: 2.0,
            thick: 3.0,
        }
    }
}

impl Default for CygnusBorders {
    /// Bordures par défaut.
    fn default() -> Self {
        Self::dark()
    }
}

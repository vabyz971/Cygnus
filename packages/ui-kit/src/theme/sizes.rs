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

//! Tailles de contrôles du thème Cygnus.
//!
//! SEULE source des hauteurs de widgets (boutons, inputs, onglets) :
//! les composants référencent ces tokens au lieu de littéraux `f32`.
//! Les espacements restent dans [`super::spacing`].

/// Hauteurs standard des contrôles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusSizes {
    /// Hauteur d'un petit bouton (24px).
    pub button_sm: f32,
    /// Hauteur d'un bouton standard (32px).
    pub button_md: f32,
    /// Hauteur d'un grand bouton (40px).
    pub button_lg: f32,
    /// Hauteur d'un champ de saisie (30px).
    pub input_height: f32,
    /// Taille d'un bouton icône carré (28px).
    pub icon_button: f32,
    /// Hauteur d'un onglet (28px).
    pub tab_height: f32,
    /// Cible tactile minimale (24px).
    pub min_touch: f32,
}

impl CygnusSizes {
    /// Tailles du thème sombre.
    pub fn dark() -> Self {
        Self {
            button_sm: 24.0,
            button_md: 32.0,
            button_lg: 40.0,
            input_height: 30.0,
            icon_button: 28.0,
            tab_height: 28.0,
            min_touch: 24.0,
        }
    }
}

impl Default for CygnusSizes {
    /// Tailles par défaut.
    fn default() -> Self {
        Self::dark()
    }
}

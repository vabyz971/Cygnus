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

//! Thème egui partagé de Cygnus : tokens, typographie et application.
//!
//! SEULE source de vérité pour les couleurs, espacements, rayons,
//! tailles de texte, tailles de contrôles et bordures des 3 apps.
//! Aucune couleur ni dimension codée en dur ailleurs.

pub mod borders;
pub mod colors;
pub mod radius;
pub mod sizes;
pub mod spacing;
// Nom imposé par la structure cible : `theme/theme.rs`.
#[allow(clippy::module_inception)]
pub mod theme;
pub mod tokens;
pub mod typography;
pub mod visuals;

pub use borders::CygnusBorders;
pub use colors::CygnusColors;
pub use radius::CygnusRadius;
pub use sizes::CygnusSizes;
pub use spacing::CygnusSpacing;
pub use theme::CygnusTheme;
pub use typography::CygnusTypography;
pub use visuals::{apply_cygnus_theme, setup_fonts};

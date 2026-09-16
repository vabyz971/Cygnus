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

//! Façade de compatibilité des tokens.
//!
//! Les structs vivent désormais dans leur module dédié
//! ([`super::colors`], [`super::spacing`], [`super::radius`],
//! [`super::typography`], [`super::sizes`], [`super::borders`],
//! [`super::theme`]). Ce module les réexporte pour préserver les
//! imports existants (`crate::theme::tokens::CygnusTheme`). Le code
//! neuf doit préférer `crate::theme::{CygnusTheme, ...}`.

pub use super::borders::CygnusBorders;
pub use super::colors::CygnusColors;
pub use super::radius::CygnusRadius;
pub use super::sizes::CygnusSizes;
pub use super::spacing::CygnusSpacing;
pub use super::theme::CygnusTheme;
pub use super::typography::CygnusTypography;

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

//! Conteneurs agnostiques : organisation sans connaissance métier.
//!
//! [`Section`] (titre nu), [`Card`] (surface), [`Stack`] (pile),
//! [`Panel`] (bloc titré) et [`Split`] (deux volets
//! redimensionnables). La disposition globale reste propre à chaque
//! app ; l'état persistant des régions vit dans [`crate::layout`].

pub mod card;
pub mod panel;
pub mod section;
pub mod split;
pub mod stack;

pub use card::Card;
pub use panel::Panel;
pub use section::Section;
pub use split::Split;
pub use stack::Stack;

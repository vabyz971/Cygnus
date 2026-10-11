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

//! Agnostic containers: organization with no domain knowledge.
//!
//! [`Section`] (bare title), [`Card`] (surface), [`Stack`] (stack),
//! [`Panel`] (titled block), [`Split`] (two resizable panes),
//! [`Collapsible`] (foldable section) and [`Toolbar`] (toolbar).
//! Overall layout stays per-app; the regions' persistent state
//! lives in [`crate::layout`].

pub mod card;
pub mod collapsible;
pub mod panel;
pub mod section;
pub mod split;
pub mod stack;
pub mod toolbar;

pub use card::Card;
pub use collapsible::Collapsible;
pub use panel::Panel;
pub use section::Section;
pub use split::Split;
pub use stack::Stack;
pub use toolbar::Toolbar;

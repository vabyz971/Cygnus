// Cygnus — Professional open-source creative suite
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

//! Photo app domain features, organized by domain.
//!
//! Each feature is independent: position-independent panels,
//! actions bubbled as [`PhotoAction`](crate::commands::PhotoAction),
//! cygnus-ui-only styling.

pub mod histogram;
pub mod history;
pub mod inspector;
pub mod layers;

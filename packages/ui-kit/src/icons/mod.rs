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

//! Icônes du design system : enum stable + registre.
//!
//! Les apps utilisent [`Icon`] et [`IconRegistry`], jamais une
//! bibliothèque d'icônes directement. La résolution passe par
//! [`CygnusIcon`](crate::widgets::icon::CygnusIcon), seul contact du
//! workspace avec `egui_material_icons`.

pub mod icon;
pub mod registry;

pub use icon::Icon;
pub use registry::IconRegistry;

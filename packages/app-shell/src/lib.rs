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

//! Squelette d'app générique : file d'actions et logique de dock.
//!
//! `app-shell` porte ce que les apps photo, video et audio partagent
//! sans connaître aucun métier : une file d'actions drainée une fois
//! par frame ([`ActionQueue`]) et la mécanique des tuiles ancrables
//! `egui_tiles` ([`dock`]). Les contenus des panneaux (titres,
//! widgets métier) restent dans chaque app.
//!
//! INTERDIT ici : toute dépendance aux engines et aux apps, et tout
//! type métier (documents, calques, clips, pistes).

pub mod actions;
pub mod dock;

pub use actions::ActionQueue;
pub use dock::DockTab;

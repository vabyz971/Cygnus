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

//! Fonctionnalités métier de l'app Photo, organisées par domaine.
//!
//! Chaque feature est indépendante : panneaux position-indépendants,
//! actions remontées en [`PhotoAction`](crate::commands::PhotoAction),
//! style exclusivement ui-kit.

pub mod inspector;
pub mod layers;

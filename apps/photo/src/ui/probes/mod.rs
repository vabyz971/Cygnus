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

//! Sondes de mesure perf (diagnostic temporaire, tests uniquement).
//!
//! Regroupe `perf_probe` (campagne composite chronométrée) et
//! `matrix_probe` (matrices de mesure) : même rôle, même durée de
//! vie. Le parent (`ui`) ne déclare ce module qu'en `#[cfg(test)]` :
//! ne JAMAIS brancher au runtime (voir chaque module).

pub mod matrix_probe;
pub mod perf_probe;

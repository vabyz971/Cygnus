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

//! Viewport photo : composition autour du viewport générique.
//!
//! Couches séparées, de bas en haut :
//!
//! - [`canvas`] : `PhotoCanvas` (texture moteur + outils, pan/zoom
//!   state-only), sans aucun envoi worker ;
//! - [`overlays`] : marqueur photo (origine) via les helpers
//!   agnostiques ui-kit.
//!
//! La caméra générique vit dans ui-kit ; le rendu et la sélection
//! photo restent ici, sans logique métier moteur.

pub mod canvas;
pub mod overlays;

pub use canvas::{PaintRequest, PhotoBrushSettings, PhotoCanvas, PhotoCanvasTool};
pub use overlays::draw_origin_marker;

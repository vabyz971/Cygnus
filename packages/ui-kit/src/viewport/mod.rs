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

//! Viewport GÉNÉRIQUE : état (zoom, pan) et widget d'affichage.
//!
//! Le viewport affiche une texture produite par un moteur (via
//! [`ViewportTextureCache`](viewport::ViewportTextureCache), alimenté
//! par l'app depuis son channel moteur) et applique zoom/pan/grille au
//! moment du draw — jamais en régénérant des pixels (rendu
//! « state-only », voir ARCHITECTURE.md).
//!
//! ui-kit ne sait PAS ce que contient la texture : photo, video ou
//! audio. Le rendu GPU réel (enregistrement natif wgpu) est câblé
//! côté app (eframe) : ui-kit ne porte que l'id de texture.
//!
//! Les [`camera`], [`pan_zoom`], [`grid`] et [`overlay`] sont
//! totalement agnostiques (aucun type PhotoDocument, Layer, Shape…) :
//! ils ne manipulent que des coordonnées monde / écran.

pub mod camera;
pub mod grid;
pub mod overlay;
pub mod pan_zoom;
pub mod viewport_interaction;
pub mod viewport_state;
// Nom imposé par la structure cible v2 (§4.1) : `viewport/viewport.rs`.
#[allow(clippy::module_inception)]
pub mod viewport;

pub use camera::Camera;
pub use grid::GridConfig;
pub use overlay::{draw_crosshair, draw_rect_world, draw_selection};
pub use pan_zoom::PanZoom;

pub use viewport::{Viewport, ViewportResponse, ViewportTextureCache, load_texture};
pub use viewport_interaction::{ViewportAction, ViewportTool, zoom_factor_for_scroll};
pub use viewport_state::ViewportState;

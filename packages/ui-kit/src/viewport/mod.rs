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

//! GENERIC viewport: state (zoom, pan) and display widget.
//!
//! The viewport displays a texture produced by an engine (via
//! [`ViewportTextureCache`](viewport::ViewportTextureCache), fed
//! by the app from its engine channel) and applies zoom/pan/grid at
//! draw time — never by regenerating pixels ("state-only"
//! rendering, see ARCHITECTURE.md).
//!
//! ui-kit does NOT know what the texture holds: photo, video or
//! audio. Real GPU rendering (native wgpu registration) is wired
//! on the app side (eframe): ui-kit only carries the texture id.
//!
//! [`camera`], [`grid`] and [`overlay`] are fully agnostic
//! (no PhotoDocument, Layer, Shape… types): they only manipulate
//! world / screen coordinates. The canonical source of the
//! zoom/pan state is [`ViewportState`]; [`Camera`] is merely a
//! projection of it (see [`Camera::from_viewport`]) used by the
//! grid and the overlays.

pub mod camera;
pub mod grid;
pub mod overlay;
pub mod viewport_interaction;
pub mod viewport_state;
// Name imposed by the target v2 structure (§4.1): `viewport/viewport.rs`.
#[allow(clippy::module_inception)]
pub mod viewport;

pub use camera::Camera;
pub use grid::GridConfig;
pub use overlay::{draw_crosshair, draw_rect_world, draw_selection};

pub use viewport::{Viewport, ViewportResponse, ViewportTextureCache, load_texture};
pub use viewport_interaction::{ViewportAction, ViewportTool, zoom_factor_for_scroll};
pub use viewport_state::ViewportState;

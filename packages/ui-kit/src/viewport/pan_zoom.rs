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

//! Contrôleur pan/zoom au-dessus d'une [`Camera`](super::camera::Camera).
//!
//! Logique pure et bornée (zoom clampé) : les gestes egui (molette,
//! drag) appellent [`PanZoom::pan_by`], [`PanZoom::zoom_by`] ou
//! [`PanZoom::zoom_at`]. Complète [`ViewportState`](super::viewport_state::ViewportState)
//! (offset écran) par une variante centrée monde ; convergence
//! prévue, sans casse de l'existant.

use super::camera::Camera;

/// Bornes de zoom par défaut (10% .. 3200%).
pub const DEFAULT_MIN_ZOOM: f32 = 0.1;
/// Borne haute de zoom par défaut.
pub const DEFAULT_MAX_ZOOM: f32 = 32.0;

/// Contrôleur pan/zoom borné.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanZoom {
    camera: Camera,
    min_zoom: f32,
    max_zoom: f32,
}

impl PanZoom {
    /// Contrôleur avec bornes personnalisées (invalides → défaut).
    pub fn new(min_zoom: f32, max_zoom: f32) -> Self {
        let valid = min_zoom > 0.0 && max_zoom >= min_zoom;
        Self {
            camera: Camera::new(),
            min_zoom: if valid { min_zoom } else { DEFAULT_MIN_ZOOM },
            max_zoom: if valid { max_zoom } else { DEFAULT_MAX_ZOOM },
        }
    }

    /// Caméra pilotée.
    pub fn camera(self) -> Camera {
        self.camera
    }

    /// Zoom courant.
    pub fn zoom(self) -> f32 {
        self.camera.zoom()
    }

    /// Pan en pixels écran (positif = contenu suit le curseur).
    pub fn pan_by(&mut self, delta_screen: egui::Vec2) {
        let zoom = self.camera.zoom().max(f32::EPSILON);
        let mut center = self.camera.center();
        center -= delta_screen / zoom;
        self.camera.set_center(center);
    }

    /// Zoom centré sur le centre de la vue.
    pub fn zoom_by(&mut self, factor: f32) {
        self.set_zoom(self.camera.zoom() * factor);
    }

    /// Zoom ancré sur `anchor_screen` (le point monde sous l'ancre
    /// reste fixe). `viewport_size` nul → zoom simple non ancré.
    pub fn zoom_at(&mut self, factor: f32, anchor_screen: egui::Pos2, viewport_size: egui::Vec2) {
        if viewport_size.x <= 0.0 || viewport_size.y <= 0.0 {
            self.zoom_by(factor);
            return;
        }
        let before = self.camera.screen_to_world(anchor_screen, viewport_size);
        self.zoom_by(factor);
        let after = self.camera.screen_to_world(anchor_screen, viewport_size);
        let mut center = self.camera.center();
        center += before - after;
        self.camera.set_center(center);
    }

    /// Réinitialise (centre origine, zoom 1).
    pub fn reset(&mut self) {
        self.camera = Camera::new();
    }

    /// Cadre un contenu monde (voir [`Camera::fit`]).
    pub fn fit(&mut self, content: egui::Rect, viewport_size: egui::Vec2, padding: f32) {
        self.camera.fit(content, viewport_size, padding);
        self.set_zoom(self.camera.zoom());
    }

    /// Applique un zoom en le bornant.
    fn set_zoom(&mut self, zoom: f32) {
        if zoom.is_finite() {
            self.camera
                .set_zoom(zoom.clamp(self.min_zoom, self.max_zoom));
        }
    }
}

impl Default for PanZoom {
    /// Contrôleur avec bornes par défaut.
    fn default() -> Self {
        Self::new(DEFAULT_MIN_ZOOM, DEFAULT_MAX_ZOOM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_is_clamped() {
        let mut pan = PanZoom::new(0.5, 4.0);
        pan.zoom_by(100.0);
        assert_eq!(pan.zoom(), 4.0);
        pan.zoom_by(0.0001);
        assert_eq!(pan.zoom(), 0.5);
    }

    #[test]
    fn invalid_bounds_fall_back_to_defaults() {
        let pan = PanZoom::new(-1.0, 0.0);
        assert_eq!(pan, PanZoom::default());
    }

    #[test]
    fn zoom_at_keeps_anchor_stable() {
        let mut pan = PanZoom::default();
        let size = egui::vec2(800.0, 600.0);
        let anchor = egui::pos2(600.0, 100.0);
        let before = pan.camera().screen_to_world(anchor, size);
        pan.zoom_at(2.0, anchor, size);
        let after = pan.camera().screen_to_world(anchor, size);
        assert!((before.x - after.x).abs() < 1e-4);
        assert!((before.y - after.y).abs() < 1e-4);
    }

    #[test]
    fn pan_moves_center_opposite_to_drag() {
        let mut pan = PanZoom::default();
        pan.pan_by(egui::vec2(100.0, 0.0));
        assert_eq!(pan.camera().center(), egui::vec2(-100.0, 0.0));
    }
}

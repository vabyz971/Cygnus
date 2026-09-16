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

//! Caméra 2D agnostique : conversions monde ↔ écran.
//!
//! Logique pure, sans type métier (ni document, ni calque, ni forme).
//! Convention : `écran = (monde - centre) * zoom + taille_vue / 2`.
//! Le zoom/pan s'appliquent au draw (modèle « state-only »).

/// Caméra 2D (centre monde + zoom).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Point monde affiché au centre de la vue.
    center: egui::Vec2,
    /// Facteur de zoom (> 0).
    zoom: f32,
}

impl Camera {
    /// Caméra identité (centre origine, zoom 1).
    pub fn new() -> Self {
        Self {
            center: egui::Vec2::ZERO,
            zoom: 1.0,
        }
    }

    /// Centre monde courant.
    pub fn center(self) -> egui::Vec2 {
        self.center
    }

    /// Zoom courant.
    pub fn zoom(self) -> f32 {
        self.zoom
    }

    /// Convertit un point monde en point écran.
    pub fn world_to_screen(self, world: egui::Pos2, viewport_size: egui::Vec2) -> egui::Pos2 {
        egui::pos2(
            (world.x - self.center.x) * self.zoom + viewport_size.x / 2.0,
            (world.y - self.center.y) * self.zoom + viewport_size.y / 2.0,
        )
    }

    /// Convertit un point écran en point monde.
    pub fn screen_to_world(self, screen: egui::Pos2, viewport_size: egui::Vec2) -> egui::Pos2 {
        let zoom = self.zoom.max(f32::EPSILON);
        egui::pos2(
            (screen.x - viewport_size.x / 2.0) / zoom + self.center.x,
            (screen.y - viewport_size.y / 2.0) / zoom + self.center.y,
        )
    }

    /// Convertit un rectangle monde en rectangle écran.
    pub fn rect_to_screen(self, world: egui::Rect, viewport_size: egui::Vec2) -> egui::Rect {
        egui::Rect::from_two_pos(
            self.world_to_screen(world.min, viewport_size),
            self.world_to_screen(world.max, viewport_size),
        )
    }

    /// Déplace le centre en coordonnées monde.
    pub fn set_center(&mut self, center: egui::Vec2) {
        self.center = center;
    }

    /// Change le zoom (valeurs non strictement positives ignorées).
    pub fn set_zoom(&mut self, zoom: f32) {
        if zoom > 0.0 && zoom.is_finite() {
            self.zoom = zoom;
        }
    }

    /// Cadre `content` (monde) dans `viewport_size` avec `padding`.
    /// Tailles nulles ou négatives : sans effet.
    pub fn fit(&mut self, content: egui::Rect, viewport_size: egui::Vec2, padding: f32) {
        if content.is_negative()
            || content.width() <= 0.0
            || content.height() <= 0.0
            || viewport_size.x <= 0.0
            || viewport_size.y <= 0.0
        {
            return;
        }
        let available =
            (viewport_size - egui::vec2(padding, padding) * 2.0).max(egui::Vec2::splat(1.0));
        let zoom = (available.x / content.width()).min(available.y / content.height());
        if zoom > 0.0 && zoom.is_finite() {
            self.zoom = zoom;
            self.center = content.center().to_vec2();
        }
    }
}

impl Default for Camera {
    /// Caméra identité.
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_screen_roundtrip() {
        let camera = Camera::new();
        let size = egui::vec2(800.0, 600.0);
        let world = egui::pos2(123.0, -45.0);
        let back = camera.screen_to_world(camera.world_to_screen(world, size), size);
        assert!((back.x - world.x).abs() < 1e-4);
        assert!((back.y - world.y).abs() < 1e-4);
    }

    #[test]
    fn origin_maps_to_viewport_center() {
        let camera = Camera::new();
        let size = egui::vec2(800.0, 600.0);
        assert_eq!(
            camera.world_to_screen(egui::Pos2::ZERO, size),
            egui::pos2(400.0, 300.0)
        );
    }

    #[test]
    fn invalid_zoom_is_ignored() {
        let mut camera = Camera::new();
        camera.set_zoom(0.0);
        camera.set_zoom(f32::INFINITY);
        assert_eq!(camera.zoom(), 1.0);
        camera.set_zoom(2.0);
        assert_eq!(camera.zoom(), 2.0);
    }

    #[test]
    fn fit_centers_content() {
        let mut camera = Camera::new();
        let content = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        camera.fit(content, egui::vec2(800.0, 600.0), 0.0);
        assert_eq!(camera.zoom(), 6.0);
        assert_eq!(camera.center(), egui::vec2(50.0, 50.0));
    }

    #[test]
    fn fit_ignores_degenerate_input() {
        let mut camera = Camera::new();
        camera.fit(egui::Rect::NOTHING, egui::vec2(800.0, 600.0), 0.0);
        camera.fit(
            egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(10.0, 10.0)),
            egui::Vec2::ZERO,
            0.0,
        );
        assert_eq!(camera, Camera::new());
    }
}

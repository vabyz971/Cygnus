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

//! État du viewport canvas : zoom, bornes et décalage (pan).
//!
//! Logique pure, sans dépendance au moteur ni au GPU : `zoom` et
//! `offset` sont appliqués au moment du draw (modèle « state-only »).
//! Transformation écran : `screen = world * zoom + offset`.

/// Bornes de zoom par défaut (10% .. 800%).
pub const DEFAULT_MIN_ZOOM: f32 = 0.1;
/// Borne haute de zoom par défaut.
pub const DEFAULT_MAX_ZOOM: f32 = 8.0;

/// État du viewport canvas (zoom + pan).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewportState {
    zoom: f32,
    min_zoom: f32,
    max_zoom: f32,
    offset: egui::Vec2,
}

impl Default for ViewportState {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            min_zoom: DEFAULT_MIN_ZOOM,
            max_zoom: DEFAULT_MAX_ZOOM,
            offset: egui::Vec2::ZERO,
        }
    }
}

impl ViewportState {
    /// Crée un viewport avec bornes personnalisées.
    /// Bornes invalides (`min <= 0`, `max < min`) → bornes par défaut.
    pub fn new(min_zoom: f32, max_zoom: f32) -> Self {
        let valid = min_zoom > 0.0 && max_zoom >= min_zoom;
        Self {
            zoom: 1.0,
            min_zoom: if valid { min_zoom } else { DEFAULT_MIN_ZOOM },
            max_zoom: if valid { max_zoom } else { DEFAULT_MAX_ZOOM },
            offset: egui::Vec2::ZERO,
        }
    }

    /// Facteur de zoom courant.
    pub fn zoom(self) -> f32 {
        self.zoom
    }

    /// Borne basse du zoom.
    pub fn min_zoom(self) -> f32 {
        self.min_zoom
    }

    /// Borne haute du zoom.
    pub fn max_zoom(self) -> f32 {
        self.max_zoom
    }

    /// Décalage de pan courant (pixels écran).
    pub fn offset(self) -> egui::Vec2 {
        self.offset
    }

    /// Définit le zoom (clampé aux bornes). Si `anchor` est donné
    /// (position écran), le point sous l'ancre reste fixe.
    /// Un zoom `NaN` est ignoré (garde-fou anti-corruption d'état).
    pub fn set_zoom(&mut self, zoom: f32, anchor: Option<egui::Pos2>) {
        if zoom.is_nan() {
            return;
        }
        let old = self.zoom;
        self.zoom = zoom.clamp(self.min_zoom, self.max_zoom);
        if let Some(anchor) = anchor
            && old > 0.0
        {
            let ratio = self.zoom / old;
            self.offset = anchor - (anchor - self.offset) * ratio;
        }
    }

    /// Multiplie le zoom par `factor` (ignoré si `<= 0`).
    pub fn zoom_by(&mut self, factor: f32, anchor: Option<egui::Pos2>) {
        if factor > 0.0 {
            self.set_zoom(self.zoom * factor, anchor);
        }
    }

    /// Définit le zoom en gardant fixe le point image sous `anchor`.
    ///
    /// Contrairement à [`set_zoom`](Self::set_zoom) (ancre exprimée
    /// dans le repère écran brut `screen = world * zoom + offset`), ici
    /// l'ancre est exprimée en coordonnées écran absolues et le
    /// viewport est centré sur `viewport_center` (géométrie réelle du
    /// widget : `screen = center + offset + world * zoom`). Sans cette
    /// correction, le zoom molette dérive vers le centre au lieu de
    /// rester sous la souris.
    pub fn set_zoom_around(&mut self, zoom: f32, anchor: egui::Pos2, viewport_center: egui::Pos2) {
        if !zoom.is_finite() {
            return;
        }
        let old = self.zoom;
        self.zoom = zoom.clamp(self.min_zoom, self.max_zoom);
        if old > 0.0 {
            let ratio = self.zoom / old;
            let rel = egui::vec2(
                anchor.x - viewport_center.x - self.offset.x,
                anchor.y - viewport_center.y - self.offset.y,
            );
            self.offset += rel - rel * ratio;
        }
    }

    /// Multiplie le zoom par `factor` en gardant fixe le point image
    /// sous `anchor` (voir [`set_zoom_around`](Self::set_zoom_around)).
    /// Ignoré si `factor <= 0` ou non fini.
    pub fn zoom_by_around(&mut self, factor: f32, anchor: egui::Pos2, viewport_center: egui::Pos2) {
        if factor > 0.0 && factor.is_finite() {
            self.set_zoom_around(self.zoom * factor, anchor, viewport_center);
        }
    }

    /// Décale la vue de `delta` pixels écran.
    pub fn pan_by(&mut self, delta: egui::Vec2) {
        self.offset += delta;
    }

    /// Réinitialise zoom (1.0) et pan (zéro).
    pub fn reset(&mut self) {
        self.zoom = 1.0;
        self.offset = egui::Vec2::ZERO;
    }

    /// Convertit des coordonnées monde (image) en pixels écran.
    pub fn world_to_screen(self, world: egui::Vec2) -> egui::Pos2 {
        egui::pos2(
            world.x * self.zoom + self.offset.x,
            world.y * self.zoom + self.offset.y,
        )
    }

    /// Convertit des pixels écran en coordonnées monde (image).
    pub fn screen_to_world(self, screen: egui::Pos2) -> egui::Vec2 {
        egui::vec2(
            (screen.x - self.offset.x) / self.zoom,
            (screen.y - self.offset.y) / self.zoom,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zoom_bounds() {
        let mut viewport = ViewportState::default();
        viewport.set_zoom(100.0, None);
        assert_eq!(viewport.zoom(), viewport.max_zoom());
        viewport.set_zoom(0.0001, None);
        assert_eq!(viewport.zoom(), viewport.min_zoom());
        viewport.set_zoom(2.0, None);
        assert_eq!(viewport.zoom(), 2.0);
    }

    #[test]
    fn zoom_ignores_invalid_factors() {
        let mut viewport = ViewportState::default();
        viewport.zoom_by(0.0, None);
        viewport.zoom_by(-2.0, None);
        viewport.zoom_by(f32::NAN, None);
        assert_eq!(viewport.zoom(), 1.0);
    }

    #[test]
    fn zoom_anchor_keeps_point_fixed() {
        let mut viewport = ViewportState::default();
        let anchor = egui::pos2(400.0, 300.0);
        let before = viewport.screen_to_world(anchor);
        viewport.zoom_by(2.0, Some(anchor));
        let after = viewport.screen_to_world(anchor);
        assert!((before.x - after.x).abs() < 1e-4);
        assert!((before.y - after.y).abs() < 1e-4);
    }

    #[test]
    fn zoom_around_keeps_image_point_under_cursor() {
        // Géométrie réelle du widget : dest centré sur `center + offset`.
        let mut viewport = ViewportState::default();
        let center = egui::pos2(200.0, 150.0);
        let anchor = egui::pos2(260.0, 110.0);
        // Point image sous le curseur avant zoom.
        let before = egui::vec2(
            (anchor.x - center.x - viewport.offset().x) / viewport.zoom(),
            (anchor.y - center.y - viewport.offset().y) / viewport.zoom(),
        );
        viewport.zoom_by_around(2.0, anchor, center);
        let after = egui::vec2(
            (anchor.x - center.x - viewport.offset().x) / viewport.zoom(),
            (anchor.y - center.y - viewport.offset().y) / viewport.zoom(),
        );
        assert!((before.x - after.x).abs() < 1e-4);
        assert!((before.y - after.y).abs() < 1e-4);
    }

    #[test]
    fn zoom_around_ignores_invalid_factors() {
        let mut viewport = ViewportState::default();
        viewport.zoom_by_around(0.0, egui::pos2(10.0, 10.0), egui::pos2(0.0, 0.0));
        viewport.zoom_by_around(-2.0, egui::pos2(10.0, 10.0), egui::pos2(0.0, 0.0));
        viewport.zoom_by_around(f32::NAN, egui::pos2(10.0, 10.0), egui::pos2(0.0, 0.0));
        assert_eq!(viewport.zoom(), 1.0);
    }

    #[test]
    fn pan_updates_offset() {
        let mut viewport = ViewportState::default();
        viewport.pan_by(egui::vec2(30.0, -12.0));
        assert_eq!(viewport.offset(), egui::vec2(30.0, -12.0));
        viewport.pan_by(egui::vec2(-30.0, 12.0));
        assert_eq!(viewport.offset(), egui::Vec2::ZERO);
    }

    #[test]
    fn world_screen_roundtrip() {
        let mut viewport = ViewportState::default();
        viewport.set_zoom(2.5, None);
        viewport.pan_by(egui::vec2(100.0, -50.0));
        let world = egui::vec2(42.0, 17.0);
        let back = viewport.screen_to_world(viewport.world_to_screen(world));
        assert!((back.x - world.x).abs() < 1e-4);
        assert!((back.y - world.y).abs() < 1e-4);
    }
}

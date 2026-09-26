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

//! Frontière de rendu vectoriel : le trait [`VectorBackend`].
//!
//! Le modèle ne connaît ni `wgpu`, ni Vello, ni aucun tessellateur : un
//! adaptateur futur (Vello ou autre) implémente ce trait avec SES propres
//! types en interne et convertit les [`Shape`](crate::Shape) à la soumission.
//! [`render_scene`] pilote l'ordre et saute les invisibles — même contrat
//! que `render-graph::execute`, sans pixels ici.

use crate::document::Shape;
use crate::{ShapeId, VectorScene};

/// Backend de rendu vectoriel (CPU, Vello ou autre — spécialisé plus tard).
///
/// Le backend convertit les formes à la soumission ; le modèle reste
/// indépendant du moteur de dessin choisi.
pub trait VectorBackend {
    /// Ouvre une passe (dimensions de la cible).
    fn begin_frame(&mut self, _width: u32, _height: u32) {}

    /// Dessine UNE forme (géométrie + styles + placement convertis par
    /// l'implémentation).
    ///
    /// # Errors
    ///
    /// Retourne un message si la soumission échoue.
    fn draw_shape(&mut self, shape: &Shape) -> Result<(), String>;

    /// Ferme la passe (présentation — sémantique du backend).
    fn end_frame(&mut self) {}
}

/// Soumet une scène en ordre de peinture, invisibles sautés.
/// Retourne le nombre de formes soumises.
///
/// # Errors
///
/// Remonte la première erreur du backend (l'état partiel appartient au
/// backend, jamais au modèle).
pub fn render_scene<B: VectorBackend>(
    backend: &mut B,
    scene: &VectorScene,
    width: u32,
    height: u32,
) -> Result<usize, String> {
    backend.begin_frame(width, height);
    let mut submitted = 0;
    for id in scene.order() {
        if let Some(shape) = scene.find(*id)
            && shape.visible
        {
            backend.draw_shape(shape)?;
            submitted += 1;
        }
    }
    backend.end_frame();
    Ok(submitted)
}

/// Backend nul : enregistre les ids soumis, ne dessine rien.
/// Gabarit des futurs backends + tests sans GPU.
#[derive(Debug, Default)]
pub struct NullBackend {
    drawn: Vec<ShapeId>,
    frames: u32,
}

impl NullBackend {
    /// Nouveau backend nul.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ids soumis, en ordre.
    #[must_use]
    pub fn drawn(&self) -> &[ShapeId] {
        &self.drawn
    }

    /// Passes ouvertes.
    #[must_use]
    pub fn frames(&self) -> u32 {
        self.frames
    }
}

impl VectorBackend for NullBackend {
    fn begin_frame(&mut self, _width: u32, _height: u32) {
        self.frames += 1;
    }

    fn draw_shape(&mut self, shape: &Shape) -> Result<(), String> {
        self.drawn.push(shape.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Fill, ShapeGeometry};
    use datatypes::Rect;

    #[test]
    fn backend_recoit_visibles_en_ordre() {
        let mut scene = VectorScene::new();
        let a = scene.add_shape(
            "a",
            ShapeGeometry::Rect(Rect::from_xywh(0.0, 0.0, 4.0, 4.0)),
        );
        let b = scene.add_shape(
            "b",
            ShapeGeometry::Rect(Rect::from_xywh(1.0, 1.0, 2.0, 2.0)),
        );
        let c = scene.add_shape(
            "c",
            ShapeGeometry::Rect(Rect::from_xywh(2.0, 2.0, 1.0, 1.0)),
        );
        assert!(scene.set_fill(a, Some(Fill::Solid(crate::Color::BLACK))));
        assert!(scene.set_visible(b, false));
        let mut backend = NullBackend::new();
        let n = render_scene(&mut backend, &scene, 64, 64).expect("null ok");
        assert_eq!(n, 2);
        assert_eq!(backend.drawn(), &[a, c]);
        assert_eq!(backend.frames(), 1);
    }
}

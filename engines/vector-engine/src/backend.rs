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

//! Vector rendering boundary: the [`VectorBackend`] trait.
//!
//! The model knows neither `wgpu`, nor Vello, nor any tessellator: a
//! future adapter (Vello or other) implements this trait with ITS OWN
//! internal types and converts [`Shape`](crate::Shape)s at submission.
//! [`render_scene`] drives the order and skips invisible shapes — same contract
//! as `render-graph::execute`, no pixels here.

use crate::document::Shape;
use crate::{ShapeId, VectorScene};

/// Vector rendering backend (CPU, Vello or other — specialized later).
///
/// The backend converts shapes at submission; the model stays
/// independent of the chosen drawing engine.
pub trait VectorBackend {
    /// Opens a pass (target dimensions).
    fn begin_frame(&mut self, _width: u32, _height: u32) {}

    /// Draws ONE shape (geometry + styles + placement converted by
    /// the implementation).
    ///
    /// # Errors
    ///
    /// Returns a message if submission fails.
    fn draw_shape(&mut self, shape: &Shape) -> Result<(), String>;

    /// Closes the pass (presentation — backend semantics).
    fn end_frame(&mut self) {}
}

/// Submits a scene in paint order, invisible shapes skipped.
/// Returns the number of submitted shapes.
///
/// # Errors
///
/// Propagates the backend's first error (partial state belongs to the
/// backend, never to the model).
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

/// Null backend: records submitted ids, draws nothing.
/// Template for future backends + GPU-less tests.
#[derive(Debug, Default)]
pub struct NullBackend {
    drawn: Vec<ShapeId>,
    frames: u32,
}

impl NullBackend {
    /// New null backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Submitted ids, in order.
    #[must_use]
    pub fn drawn(&self) -> &[ShapeId] {
        &self.drawn
    }

    /// Open passes.
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

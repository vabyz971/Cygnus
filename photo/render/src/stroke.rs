// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Preview of a stroke while it is drawn (docs/decisions/0036, step P2 of
//! J5).
//!
//! The preview keeps the document as it was when the stroke began and a
//! copy where the target shows the stroke so far, which the canvas
//! displays. Each new input recomputes the dabs of the whole stroke (the
//! end of the curve moves with the next point) and repaints, from their
//! original content, only the tiles whose dabs changed, by the very pass of
//! `brush.stroke`. When the stroke ends, [`StrokePreview::finish`] gives
//! the command with those tiles already resolved: what was shown is
//! exactly what the command writes, and a replay of the command (script,
//! agent) computes the same pixels on the same adapter.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use cygnus_commands::ChangeSet;
use cygnus_photo_core::brush::{self, Brush, BrushStroke, Dab, StrokePoint};
use cygnus_photo_core::pixels::{Tile, TileCoord};
use cygnus_photo_core::{CoreError, Document, NodeId, PhotoChanges, PhotoCommand, Rect};

use crate::brush::bins;
use crate::renderer::{RenderError, Renderer};

/// Why a preview could not go on.
#[derive(Debug, thiserror::Error)]
pub enum StrokeError {
    /// The command refuses the stroke (target, settings, points).
    #[error(transparent)]
    Refused(#[from] CoreError),
    /// The GPU failed.
    #[error(transparent)]
    Render(#[from] RenderError),
}

/// A stroke being drawn.
pub struct StrokePreview {
    target: NodeId,
    brush: Brush,
    bounds: Rect,
    /// The document when the stroke began.
    base: Arc<Document>,
    /// The document showing the stroke so far.
    shown: Document,
    points: Vec<StrokePoint>,
    dabs: Vec<Dab>,
    /// Painted tiles of the target, by coordinates.
    painted: BTreeMap<TileCoord, Option<Arc<Tile>>>,
    /// Dabs reaching each tile, by index.
    reached: BTreeMap<TileCoord, Vec<usize>>,
    /// Tiles repainted by the last update.
    repainted: usize,
}

impl StrokePreview {
    /// Starts a stroke on `target` of `document` with `brush`: refused
    /// with the error of the command for a target `brush.stroke` refuses.
    pub fn begin(document: &Document, target: NodeId, brush: Brush) -> Result<Self, StrokeError> {
        let bounds = cygnus_photo_core::commands::stroke_target(document, target)?;
        brush.validate()?;
        Ok(Self {
            target,
            brush,
            bounds,
            base: Arc::new(document.clone()),
            shown: document.clone(),
            points: Vec::new(),
            dabs: Vec::new(),
            painted: BTreeMap::new(),
            reached: BTreeMap::new(),
            repainted: 0,
        })
    }

    /// The document showing the stroke so far.
    pub fn document(&self) -> &Document {
        &self.shown
    }

    /// Points of the stroke so far.
    pub fn points(&self) -> &[StrokePoint] {
        &self.points
    }

    /// Tiles repainted by the last [`Self::extend`].
    pub fn repainted(&self) -> usize {
        self.repainted
    }

    /// Adds `points` to the stroke and repaints what they change. Returns
    /// the tiles of the shown document that changed.
    pub fn extend(
        &mut self,
        renderer: &Renderer,
        points: &[StrokePoint],
    ) -> Result<PhotoChanges, StrokeError> {
        self.repainted = 0;
        if points.is_empty() {
            return Ok(PhotoChanges::default());
        }
        self.points.extend_from_slice(points);
        let dabs = brush::dabs(&self.points, &self.brush)?;
        // Dabs before the first difference are unchanged: only the tiles
        // reached by the others, old or new, change.
        let same = self
            .dabs
            .iter()
            .zip(&dabs)
            .take_while(|(old, new)| old == new)
            .count();
        let reached = bins(&dabs, self.bounds);
        let mut dirty: BTreeSet<TileCoord> = BTreeSet::new();
        for (coord, indices) in self.reached.iter().chain(reached.iter()) {
            if indices.iter().any(|index| *index >= same) {
                dirty.insert(*coord);
            }
        }
        self.dabs = dabs;
        self.reached = reached;
        if dirty.is_empty() {
            return Ok(PhotoChanges::default());
        }
        let tiles = renderer.paint_tiles(
            &self.base,
            self.target,
            &self.brush,
            &self.dabs,
            Some(&dirty),
        )?;
        let mut update: Vec<(TileCoord, Option<Arc<Tile>>)> = Vec::with_capacity(dirty.len());
        for (coord, tile) in tiles {
            self.painted.insert(coord, tile.clone());
            update.push((coord, tile));
        }
        // Tiles no dab reaches any more get their original content back.
        for coord in &dirty {
            if !self.reached.contains_key(coord) {
                self.painted.remove(coord);
                update.push((*coord, self.base.pixels.tile(self.target, *coord).cloned()));
            }
        }
        self.repainted = update.len();
        let mut changes = PhotoChanges::default();
        changes.merge(&PhotoChanges::pixels(
            self.target,
            update.iter().map(|(coord, _)| *coord),
        ));
        self.shown.pixels.restore(self.target, update);
        Ok(changes)
    }

    /// Ends the stroke: the `brush.stroke` command, with the painted tiles
    /// as its resolution, or `None` if it paints nothing.
    pub fn finish(self) -> Option<PhotoCommand> {
        if self.painted.is_empty() {
            return None;
        }
        Some(PhotoCommand::BrushStroke(BrushStroke {
            target: self.target,
            tool: Default::default(),
            points: self.points,
            brush: self.brush,
            resolved: Some(self.painted.into_iter().collect()),
        }))
    }
}

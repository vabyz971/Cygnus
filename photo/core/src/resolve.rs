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

//! Resolved commands (docs/decisions/0020): `source.import` needs to read a
//! file, `layer.merge_down` to render two layers and `brush.stroke` to
//! render its dabs (0036), which this crate cannot do (it depends neither
//! on the disk nor on the GPU). The caller
//! gives a reader and a renderer; [`resolve_and_execute`] is the **only**
//! path that runs commands with them. The commands it resolves carry their
//! result, unserialized, and are refused by the bus without it.

use cygnus_commands::{CommandBus, Revision, Source as Origin};

use crate::brush::{self, Brush, Dab};
use crate::commands::{CoreError, ImportedSource, PhotoCommand, merge_down_target, stroke_target};
use crate::model::{Document, NodeId, Source, SourceId};
use crate::pixels::{LayerTiles, TileBackup};

/// Reads and decodes an image file (implemented by `photo/io`).
pub trait Import {
    /// The decoded file and its identifier (SHA-256 of the decoded
    /// content), or why it cannot be read.
    fn import(&self, path: &str) -> Result<(SourceId, Source), String>;
}

/// Renders two layers into one (implemented by `photo/render`).
pub trait Merge {
    /// Pixels of `upper` composed over the layer below it, alone, in the
    /// document depth, or why the render failed. Called only once the merge
    /// is known to be exact ([`merge_down_target`]).
    fn merge(&self, document: &Document, upper: NodeId) -> Result<LayerTiles, String>;
}

/// Renders the dabs of a stroke (implemented by `photo/render`).
pub trait Paint {
    /// The new content of the tiles of `target` that `dabs` reach, within
    /// the layer, in the document depth (`None` where a tile becomes
    /// empty), or why the render failed. Called only once the target is
    /// known to be an editable pixel layer and some dab reaches it.
    fn paint(
        &self,
        document: &Document,
        target: NodeId,
        brush: &Brush,
        dabs: &[Dab],
    ) -> Result<TileBackup, String>;
}

/// The reader and the renderer available to the caller.
#[derive(Clone, Copy)]
pub struct Resolvers<'a> {
    /// File reader, for `source.import`.
    pub import: Option<&'a dyn Import>,
    /// Renderer, for `layer.merge_down`.
    pub merge: Option<&'a dyn Merge>,
    /// Renderer, for `brush.stroke`.
    pub paint: Option<&'a dyn Paint>,
}

impl Resolvers<'_> {
    /// Neither reader nor renderer: resolved commands are refused.
    pub const NONE: Resolvers<'static> = Resolvers {
        import: None,
        merge: None,
        paint: None,
    };
}

fn failed(command: &'static str, reason: impl Into<String>) -> CoreError {
    CoreError::Resolution {
        command,
        reason: reason.into(),
    }
}

/// Resolves `command` if it needs a file or a render, then executes it on
/// `bus`. Every entry point (interface, command line, MCP server) runs
/// commands through this function.
pub fn resolve_and_execute(
    bus: &mut CommandBus<PhotoCommand>,
    document: &mut Document,
    command: PhotoCommand,
    source: Origin,
    resolvers: &Resolvers<'_>,
) -> Result<Revision, CoreError> {
    let command = match command {
        PhotoCommand::SourceImport(mut params) if params.resolved.is_none() => {
            let reader = resolvers
                .import
                .ok_or_else(|| failed("source.import", "no file reader is available here"))?;
            let (id, decoded) = reader
                .import(&params.path)
                .map_err(|reason| failed("source.import", reason))?;
            params.resolved = Some(ImportedSource {
                id,
                source: decoded,
            });
            PhotoCommand::SourceImport(params)
        }
        PhotoCommand::LayerMergeDown(mut params) if params.resolved.is_none() => {
            // Checked before rendering: a refused merge renders nothing.
            merge_down_target(document, params.id)?;
            let renderer = resolvers
                .merge
                .ok_or_else(|| failed("layer.merge_down", "no renderer is available here"))?;
            let tiles = renderer
                .merge(document, params.id)
                .map_err(|reason| failed("layer.merge_down", reason))?;
            params.resolved = Some(tiles);
            PhotoCommand::LayerMergeDown(params)
        }
        PhotoCommand::BrushStroke(mut params) if params.resolved.is_none() => {
            // Checked before rendering: a refused stroke renders nothing.
            let bounds = stroke_target(document, params.target)?;
            let dabs = brush::dabs(&params.points, &params.brush)?;
            if !dabs.iter().any(|dab| dab.touches(&bounds)) {
                return Err(CoreError::EmptyStroke);
            }
            let renderer = resolvers
                .paint
                .ok_or_else(|| failed("brush.stroke", "no renderer is available here"))?;
            let tiles = renderer
                .paint(document, params.target, &params.brush, &dabs)
                .map_err(|reason| failed("brush.stroke", reason))?;
            params.resolved = Some(tiles);
            PhotoCommand::BrushStroke(params)
        }
        other => other,
    };
    bus.execute(document, command, source)
}

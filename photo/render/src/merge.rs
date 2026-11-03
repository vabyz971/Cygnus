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

//! `layer.merge_down` (docs/decisions/0020): the two layers are rendered
//! alone, by the export path, into the pixels of the merged layer.

use cygnus_photo_core::pixels::{LayerTiles, PixelStore, TileFormat};
use cygnus_photo_core::resolve::Merge;
use cygnus_photo_core::{Depth, Document, NodeId, NodeKind, Rect};

use crate::image::ExportBits;
use crate::renderer::Renderer;

impl Merge for Renderer {
    fn merge(&self, document: &Document, upper: NodeId) -> Result<LayerTiles, String> {
        let below = cygnus_photo_core::commands::merge_down_target(document, upper)
            .map_err(|error| error.to_string())?;
        // A document showing only the two layers, at the root. Both in
        // Normal (checked), so composing them alone and then over the
        // backdrop equals composing them over the backdrop. Two clipped
        // layers compose over each other, then clip to their base as one
        // (A atop (B atop C) = (A over B) atop C); a layer clipped onto the
        // one below it keeps that clipping.
        let mut alone = document.clone();
        let below_clipped = document.node(below).is_some_and(|node| node.clip);
        let root = alone.root;
        let upper_clipped = document.node(upper).is_some_and(|node| node.clip);
        for (id, clip) in [(below, false), (upper, upper_clipped && !below_clipped)] {
            if let Some(node) = alone.nodes.get_mut(&id) {
                node.parent = Some(root);
                node.clip = clip;
            }
        }
        if let Some(NodeKind::Group { children, .. }) =
            alone.nodes.get_mut(&root).map(|node| &mut node.kind)
        {
            *children = vec![below, upper];
        }
        let bits = match document.depth {
            Depth::U8 => ExportBits::Eight,
            Depth::U16 => ExportBits::Sixteen,
        };
        let image = self
            .export(&alone, bits)
            .map_err(|error| error.to_string())?;
        let mut store = PixelStore::default();
        let width = image.width as usize;
        store.paint(
            below,
            Rect {
                x: 0,
                y: 0,
                width: image.width,
                height: image.height,
            },
            TileFormat::rgba(document.depth),
            |x, y| {
                let i = (y as usize * width + x as usize) * 4;
                [
                    image.data[i],
                    image.data[i + 1],
                    image.data[i + 2],
                    image.data[i + 3],
                ]
            },
        );
        Ok(store.take_layer(below))
    }
}

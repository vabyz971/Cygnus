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

//! What a Photo command changed, read by the views and the render cache.

use std::collections::{BTreeMap, BTreeSet};

use cygnus_commands::ChangeSet;

use crate::model::NodeId;
use crate::pixels::TileCoord;

/// Changes made by one or several Photo commands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PhotoChanges {
    /// Anything may have changed: refresh everything.
    pub everything: bool,
    /// Document settings changed (size, depth, profile, resolution).
    pub document: bool,
    /// The tree structure changed (nodes added, removed or moved).
    pub structure: bool,
    /// Nodes whose properties changed.
    pub properties: BTreeSet<NodeId>,
    /// Tiles whose pixels changed, per layer.
    pub pixels: BTreeMap<NodeId, BTreeSet<TileCoord>>,
    /// Nodes created, in creation order.
    pub created: Vec<NodeId>,
}

impl PhotoChanges {
    pub(crate) fn structure() -> Self {
        Self {
            structure: true,
            ..Self::default()
        }
    }

    pub(crate) fn properties(node: NodeId) -> Self {
        Self {
            properties: [node].into(),
            ..Self::default()
        }
    }

    pub(crate) fn pixels(layer: NodeId, tiles: impl IntoIterator<Item = TileCoord>) -> Self {
        Self {
            pixels: [(layer, tiles.into_iter().collect())].into(),
            ..Self::default()
        }
    }
}

impl ChangeSet for PhotoChanges {
    fn merge(&mut self, other: &Self) {
        self.everything |= other.everything;
        self.document |= other.document;
        self.structure |= other.structure;
        self.properties.extend(other.properties.iter().copied());
        for (layer, tiles) in &other.pixels {
            self.pixels
                .entry(*layer)
                .or_default()
                .extend(tiles.iter().copied());
        }
        self.created.extend(other.created.iter().copied());
    }

    fn everything() -> Self {
        Self {
            everything: true,
            ..Self::default()
        }
    }

    fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

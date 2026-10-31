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

//! CPU storage of layer pixels, the reference copy (docs/decisions/0011).
//!
//! Pixels are kept in the document depth (8 or 16 bits per channel), RGBA,
//! encoded in the document profile, with straight (not premultiplied)
//! alpha. They are split into square tiles; a tile that is entirely
//! transparent is not stored. Tiles are shared through [`Arc`], so that
//! cloning a document or keeping overwritten tiles for undo costs no copy.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::model::{Depth, NodeId, Rect};

/// Side of a tile, in pixels.
pub const TILE_SIZE: u32 = 256;

/// Channel values per tile (RGBA).
const TILE_VALUES: usize = (TILE_SIZE * TILE_SIZE * 4) as usize;

/// Position of a tile, in tiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TileCoord {
    /// Column.
    pub tx: u32,
    /// Row.
    pub ty: u32,
}

impl TileCoord {
    /// Pixel rectangle covered by the tile.
    pub fn rect(self) -> Rect {
        Rect {
            x: self.tx * TILE_SIZE,
            y: self.ty * TILE_SIZE,
            width: TILE_SIZE,
            height: TILE_SIZE,
        }
    }
}

/// Tiles overlapping `rect`, row by row.
pub fn tiles_in(rect: Rect) -> impl Iterator<Item = TileCoord> {
    let (first_x, first_y) = (rect.x / TILE_SIZE, rect.y / TILE_SIZE);
    let last =
        |edge: u64| u32::try_from(edge.saturating_sub(1) / u64::from(TILE_SIZE)).unwrap_or(0);
    let (last_x, last_y) = (last(rect.right()), last(rect.bottom()));
    let empty = rect.is_empty();
    (first_y..=last_y)
        .flat_map(move |ty| (first_x..=last_x).map(move |tx| TileCoord { tx, ty }))
        .filter(move |_| !empty)
}

/// Pixel values of one tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Tile {
    /// 8 bits per channel.
    U8(Box<[u8]>),
    /// 16 bits per channel.
    U16(Box<[u16]>),
}

impl Tile {
    /// A transparent tile.
    pub fn transparent(depth: Depth) -> Self {
        match depth {
            Depth::U8 => Self::U8(vec![0; TILE_VALUES].into_boxed_slice()),
            Depth::U16 => Self::U16(vec![0; TILE_VALUES].into_boxed_slice()),
        }
    }

    /// Memory used by the pixel values, in bytes.
    pub fn bytes(&self) -> usize {
        match self {
            Self::U8(values) => values.len(),
            Self::U16(values) => values.len() * 2,
        }
    }

    fn index(x: u32, y: u32) -> usize {
        ((y * TILE_SIZE + x) * 4) as usize
    }

    /// RGBA values of the pixel at (`x`, `y`) inside the tile.
    pub fn get(&self, x: u32, y: u32) -> [u16; 4] {
        let i = Self::index(x, y);
        let mut out = [0; 4];
        for (channel, value) in out.iter_mut().enumerate() {
            *value = match self {
                Self::U8(values) => u16::from(values[i + channel]),
                Self::U16(values) => values[i + channel],
            };
        }
        out
    }

    fn set(&mut self, x: u32, y: u32, rgba: [u16; 4]) {
        let i = Self::index(x, y);
        for (channel, value) in rgba.into_iter().enumerate() {
            match self {
                // Values are validated against the depth by the commands.
                Self::U8(values) => values[i + channel] = u8::try_from(value).unwrap_or(u8::MAX),
                Self::U16(values) => values[i + channel] = value,
            }
        }
    }

    fn is_transparent(&self) -> bool {
        match self {
            Self::U8(values) => values.iter().all(|value| *value == 0),
            Self::U16(values) => values.iter().all(|value| *value == 0),
        }
    }
}

/// Tiles of one layer.
pub type LayerTiles = BTreeMap<TileCoord, Arc<Tile>>;

/// Previous content of tiles overwritten by a change, to undo it.
pub type TileBackup = Vec<(TileCoord, Option<Arc<Tile>>)>;

/// Pixels of every layer of a document. Each tile carries its depth, which
/// is the document depth.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PixelStore {
    layers: BTreeMap<NodeId, LayerTiles>,
}

impl PixelStore {
    /// Layers that have at least one stored tile.
    pub fn layers(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.layers.keys().copied()
    }

    /// Stored tiles of `layer`.
    pub fn tiles(&self, layer: NodeId) -> impl Iterator<Item = (TileCoord, &Arc<Tile>)> {
        self.layers
            .get(&layer)
            .into_iter()
            .flat_map(|tiles| tiles.iter().map(|(coord, tile)| (*coord, tile)))
    }

    /// The tile of `layer` at `coord`; `None` means transparent.
    pub fn tile(&self, layer: NodeId, coord: TileCoord) -> Option<&Arc<Tile>> {
        self.layers.get(&layer).and_then(|tiles| tiles.get(&coord))
    }

    /// RGBA values of one pixel of `layer`.
    pub fn pixel(&self, layer: NodeId, x: u32, y: u32) -> [u16; 4] {
        let coord = TileCoord {
            tx: x / TILE_SIZE,
            ty: y / TILE_SIZE,
        };
        self.tile(layer, coord)
            .map_or([0; 4], |tile| tile.get(x % TILE_SIZE, y % TILE_SIZE))
    }

    /// Removes and returns every tile of `layer`.
    pub fn take_layer(&mut self, layer: NodeId) -> LayerTiles {
        self.layers.remove(&layer).unwrap_or_default()
    }

    /// Puts back the tiles of `layer`, replacing any existing ones.
    pub fn insert_layer(&mut self, layer: NodeId, tiles: LayerTiles) {
        if tiles.is_empty() {
            self.layers.remove(&layer);
        } else {
            self.layers.insert(layer, tiles);
        }
    }

    /// Writes the pixels of `rect` (already clipped to the layer) of `layer`
    /// with `source(x, y)`, in document coordinates; new tiles get `depth`.
    /// Returns the previous content of the touched tiles.
    pub fn paint(
        &mut self,
        layer: NodeId,
        rect: Rect,
        depth: Depth,
        mut source: impl FnMut(u32, u32) -> [u16; 4],
    ) -> TileBackup {
        let tiles = self.layers.entry(layer).or_default();
        let mut backup = Vec::new();
        for coord in tiles_in(rect) {
            let previous = tiles.get(&coord).cloned();
            let mut tile = previous
                .clone()
                .unwrap_or_else(|| Arc::new(Tile::transparent(depth)));
            let area = rect.intersect(&coord.rect());
            let target = Arc::make_mut(&mut tile);
            for y in area.y..area.y + area.height {
                for x in area.x..area.x + area.width {
                    target.set(x - coord.rect().x, y - coord.rect().y, source(x, y));
                }
            }
            if tile.is_transparent() {
                tiles.remove(&coord);
            } else {
                tiles.insert(coord, tile);
            }
            backup.push((coord, previous));
        }
        if tiles.is_empty() {
            self.layers.remove(&layer);
        }
        backup
    }

    /// Restores tiles saved by [`Self::paint`]. Returns the content they
    /// replace, to redo the change.
    pub fn restore(&mut self, layer: NodeId, backup: TileBackup) -> TileBackup {
        let tiles = self.layers.entry(layer).or_default();
        let mut replaced = Vec::with_capacity(backup.len());
        for (coord, tile) in backup {
            let current = match tile {
                Some(tile) => tiles.insert(coord, tile),
                None => tiles.remove(&coord),
            };
            replaced.push((coord, current));
        }
        if tiles.is_empty() {
            self.layers.remove(&layer);
        }
        replaced
    }
}

/// Memory retained by a backup, in bytes.
pub fn backup_bytes(backup: &TileBackup) -> usize {
    backup
        .iter()
        .filter_map(|(_, tile)| tile.as_ref())
        .map(|tile| tile.bytes())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LAYER: NodeId = NodeId(1);

    #[test]
    fn tiles_cover_the_rectangle() {
        let rect = Rect {
            x: 250,
            y: 0,
            width: 10,
            height: 300,
        };
        let coords: Vec<_> = tiles_in(rect).collect();
        assert_eq!(
            coords,
            vec![
                TileCoord { tx: 0, ty: 0 },
                TileCoord { tx: 1, ty: 0 },
                TileCoord { tx: 0, ty: 1 },
                TileCoord { tx: 1, ty: 1 },
            ]
        );
        assert_eq!(tiles_in(Rect::of_size(0, 10)).count(), 0);
    }

    #[test]
    fn paint_then_restore_is_identity() {
        let mut store = PixelStore::default();
        let first = store.paint(LAYER, Rect::of_size(300, 10), Depth::U16, |_, _| {
            [1, 2, 3, 4]
        });
        let before = store.clone();
        let backup = store.paint(LAYER, Rect::of_size(20, 20), Depth::U16, |x, _| {
            [x as u16, 0, 0, 9]
        });
        assert_ne!(store, before);
        store.restore(LAYER, backup);
        assert_eq!(store, before);
        store.restore(LAYER, first);
        assert_eq!(store, PixelStore::default());
    }

    #[test]
    fn transparent_tiles_are_not_stored() {
        let mut store = PixelStore::default();
        let area = Rect::of_size(10, 10);
        store.paint(LAYER, area, Depth::U8, |_, _| [0; 4]);
        assert_eq!(store.layers().count(), 0);
        store.paint(LAYER, area, Depth::U8, |_, _| [255; 4]);
        assert_eq!(store.tiles(LAYER).count(), 1);
        store.paint(LAYER, area, Depth::U8, |_, _| [0; 4]);
        assert_eq!(store.layers().count(), 0);
    }

    #[test]
    fn shared_tiles_are_copied_on_write() {
        let mut store = PixelStore::default();
        store.paint(LAYER, Rect::of_size(4, 4), Depth::U8, |_, _| [10; 4]);
        let snapshot = store.clone();
        store.paint(LAYER, Rect::of_size(1, 1), Depth::U8, |_, _| [20; 4]);
        assert_eq!(snapshot.pixel(LAYER, 0, 0), [10; 4]);
        assert_eq!(store.pixel(LAYER, 0, 0), [20; 4]);
    }
}

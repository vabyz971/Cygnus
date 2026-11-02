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

//! Source pools (docs/decisions/0021): layer and mask tiles uploaded as
//! integers, as stored in the document, and kept on the GPU from one
//! render to the next.
//!
//! Each tile (node, tile coordinates) gets a slot on demand. A resident
//! tile is identified by its shared allocation in the [`PixelStore`]: tiles
//! are copy-on-write, so a changed tile is a new allocation and is uploaded
//! again, while an unchanged one never is. The pool holds a reference to
//! the tile it uploaded, so that the allocation cannot be reused for another
//! tile while the slot is resident. When the pool is full, the least
//! recently used tiles that the current batch does not need are evicted.
//!
//! [`PixelStore`]: cygnus_photo_core::pixels::PixelStore

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cygnus_photo_core::NodeId;
use cygnus_photo_core::pixels::{TILE_SIZE, Tile, TileCoord};

use crate::format::{MASK_FORMAT, MASK_TEXEL_BYTES, SOURCE_FORMAT, SOURCE_TEXEL_BYTES};
use crate::renderer::RenderError;

/// A source tile: the node it belongs to and its tile coordinates.
pub(crate) type SourceKey = (NodeId, TileCoord);

/// Counters of the source pools, for tests and the benchmark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SourceStats {
    /// Tiles uploaded since the renderer was created (layers and masks).
    pub uploads: u64,
    /// Resident tiles evicted to make room.
    pub evictions: u64,
    /// Slots of the layer pool and of the mask pool.
    pub capacity: (u32, u32),
    /// Curve lookup tables computed and uploaded.
    pub curve_tables: crate::adjust::CurveTableStats,
}

/// What a pool stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PoolKind {
    /// RGBA layer tiles.
    Layers,
    /// Gray mask tiles.
    Masks,
}

struct Resident {
    slot: u32,
    tile: Arc<Tile>,
    used: u64,
}

/// One pool of source tiles in a texture array.
pub(crate) struct TilePool {
    kind: PoolKind,
    texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    capacity: u32,
    entries: HashMap<SourceKey, Resident>,
    free: Vec<u32>,
}

impl TilePool {
    pub(crate) fn new(device: &wgpu::Device, kind: PoolKind, capacity: u32) -> Self {
        let (label, format) = match kind {
            PoolKind::Layers => ("cygnus layer source pool", SOURCE_FORMAT),
            PoolKind::Masks => ("cygnus mask source pool", MASK_FORMAT),
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: capacity,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        Self {
            kind,
            texture,
            view,
            capacity,
            entries: HashMap::new(),
            free: (0..capacity).rev().collect(),
        }
    }

    pub(crate) fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Makes every tile of `wanted` resident and returns its slot. Tiles
    /// not in `wanted` may be evicted; tiles in it never are. The caller
    /// guarantees that `wanted` fits in the pool, and that GPU work recorded
    /// with earlier slots has been submitted (an upload replaces a slot
    /// before any later submission runs).
    pub(crate) fn ensure(
        &mut self,
        queue: &wgpu::Queue,
        wanted: &[(SourceKey, &Arc<Tile>)],
        clock: u64,
        stats: &mut SourceStats,
    ) -> Result<HashMap<SourceKey, u32>, RenderError> {
        if wanted.len() > self.capacity as usize {
            return Err(RenderError::TooManyLayers {
                count: wanted.len(),
            });
        }
        let pinned: HashSet<SourceKey> = wanted.iter().map(|(key, _)| *key).collect();
        let mut slots = HashMap::with_capacity(wanted.len());
        for (key, tile) in wanted {
            let slot = match self.entries.get_mut(key) {
                Some(entry) if Arc::ptr_eq(&entry.tile, tile) => {
                    entry.used = clock;
                    entry.slot
                }
                Some(entry) => {
                    // The tile changed: same slot, new content.
                    entry.tile = Arc::clone(tile);
                    entry.used = clock;
                    let slot = entry.slot;
                    self.upload(queue, slot, tile)?;
                    stats.uploads += 1;
                    slot
                }
                None => {
                    let slot = match self.free.pop() {
                        Some(slot) => slot,
                        None => self.evict(&pinned, stats)?,
                    };
                    self.upload(queue, slot, tile)?;
                    stats.uploads += 1;
                    self.entries.insert(
                        *key,
                        Resident {
                            slot,
                            tile: Arc::clone(tile),
                            used: clock,
                        },
                    );
                    slot
                }
            };
            slots.insert(*key, slot);
        }
        Ok(slots)
    }

    /// Frees the slot of the least recently used tile outside `pinned`.
    fn evict(
        &mut self,
        pinned: &HashSet<SourceKey>,
        stats: &mut SourceStats,
    ) -> Result<u32, RenderError> {
        let victim = self
            .entries
            .iter()
            .filter(|(key, _)| !pinned.contains(key))
            .min_by_key(|(key, entry)| (entry.used, **key))
            .map(|(key, _)| *key);
        let entry =
            victim
                .and_then(|key| self.entries.remove(&key))
                .ok_or(RenderError::TooManyLayers {
                    count: pinned.len(),
                })?;
        stats.evictions += 1;
        Ok(entry.slot)
    }

    fn upload(&self, queue: &wgpu::Queue, slot: u32, tile: &Tile) -> Result<(), RenderError> {
        let (texel_bytes, values) = match (self.kind, tile) {
            (PoolKind::Layers, Tile::U8(values)) | (PoolKind::Masks, Tile::Gray8(values)) => {
                // 8-bit values extended exactly to 16 bits (v × 257).
                let wide: Vec<u16> = values.iter().map(|v| u16::from(*v) * 257).collect();
                (texel_bytes(self.kind), le_bytes(&wide))
            }
            (PoolKind::Layers, Tile::U16(values)) | (PoolKind::Masks, Tile::Gray16(values)) => {
                (texel_bytes(self.kind), le_bytes(values))
            }
            _ => {
                return Err(RenderError::Unsupported(
                    "a tile of the wrong kind for its pool",
                ));
            }
        };
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d {
                    x: 0,
                    y: 0,
                    z: slot,
                },
                aspect: wgpu::TextureAspect::All,
            },
            &values,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(TILE_SIZE * texel_bytes),
                rows_per_image: Some(TILE_SIZE),
            },
            wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: 1,
            },
        );
        Ok(())
    }
}

fn texel_bytes(kind: PoolKind) -> u32 {
    match kind {
        PoolKind::Layers => SOURCE_TEXEL_BYTES,
        PoolKind::Masks => MASK_TEXEL_BYTES,
    }
}

/// Little-endian bytes of 16-bit values, as the GPU reads them.
fn le_bytes(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

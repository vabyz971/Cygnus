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

//! Tile cache: versioned slots, opaque handles.
//!
//! [`TileCache<H>`] maps `TileId → (revision, handle)`: hit iff the
//! stored revision EQUALS the requested one (conceptual [`TileKey`]). The
//! handle type `H` STRICTLY separates the worlds:
//!
//! - [`CpuTile`] (shared `Arc<[u8]>` pixels) — [`CpuTileCache`];
//! - [`GpuTile`] (opaque `token`, resolved into a texture by the backend)
//!   — [`GpuTileCache`].
//!
//! No pixel transits here toward the GPU and no readback comes back:
//! the backend owns the `token → texture` table and frees its textures
//! when `insert` hands it an evicted handle back (return value — no
//! callback, no implicit synchronization).
//!
//! Bounded LRU eviction (monotonic counter + minimum scan):
//! deliberately simple foundation — refined backend caches will do
//! better, with the same `get/insert/invalidate` interface.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{TileId, TileRevision};

/// CPU tile: shared pixels (zero copy to readers).
#[derive(Debug, Clone)]
pub struct CpuTile {
    /// Shared row-major RGBA8 pixels.
    pub data: Arc<[u8]>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// CPU tile cache.
pub type CpuTileCache = TileCache<CpuTile>;

/// Opaque GPU handle: only the backend can resolve it into a texture.
///
/// `token` is assigned by the backend at creation; `generation`
/// changes on each re-upload (detects stale tokens without
/// touching the GPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GpuTile {
    /// Token assigned by the backend.
    pub token: u64,
    /// Generation of the uploaded content.
    pub generation: u64,
}

/// GPU tile cache (tokens — textures live with the backend).
pub type GpuTileCache = TileCache<GpuTile>;

/// Versioned entry (revision + handle + LRU recency).
#[derive(Debug, Clone)]
struct CacheEntry<H> {
    revision: TileRevision,
    handle: H,
    last_used: u64,
}

/// Cache statistics (observability, no effect).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Stored entries.
    pub len: usize,
    /// Configured capacity.
    pub capacity: usize,
    /// Successful reads (same revision).
    pub hits: u64,
    /// Failed reads (absent or different revision).
    pub misses: u64,
    /// Evicted handles (handed back to the caller via `insert`).
    pub evicted: u64,
}

/// Bounded LRU cache of versioned tiles, parameterized by handle.
///
/// `H` is never inspected here (opaque storage): CPU and GPU never
/// mix — they are two instances of distinct types.
#[derive(Debug, Clone)]
pub struct TileCache<H> {
    capacity: usize,
    entries: HashMap<TileId, CacheEntry<H>>,
    clock: u64,
    hits: u64,
    misses: u64,
    evicted: u64,
}

impl<H> TileCache<H> {
    /// New cache (`capacity` = max entries; 0 = stores nothing).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: HashMap::new(),
            clock: 0,
            hits: 0,
            misses: 0,
            evicted: 0,
        }
    }

    /// Configured capacity.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Stored entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Cumulative statistics.
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        CacheStats {
            len: self.entries.len(),
            capacity: self.capacity,
            hits: self.hits,
            misses: self.misses,
            evicted: self.evicted,
        }
    }

    /// Read: hit iff the entry exists AND its revision equals `revision`.
    /// Marks LRU recency (hence `&mut`).
    #[must_use]
    pub fn get(&mut self, id: TileId, revision: TileRevision) -> Option<&H> {
        self.clock = self.clock.wrapping_add(1);
        let tick = self.clock;
        match self.entries.get_mut(&id) {
            Some(entry) if entry.revision == revision => {
                entry.last_used = tick;
                self.hits += 1;
                Some(&entry.handle)
            }
            _ => {
                self.misses += 1;
                None
            }
        }
    }

    /// Stored revision for the slot (`None` if absent).
    #[must_use]
    pub fn revision_of(&self, id: TileId) -> Option<TileRevision> {
        self.entries.get(&id).map(|e| e.revision)
    }

    /// Stores (or replaces) a handle. If the cache is full and the id is
    /// new, the LEAST recently used handle is handed back to
    /// the caller (the backend frees its texture there — no silent
    /// leak). Capacity 0: immediately hands `handle` back.
    pub fn insert(&mut self, id: TileId, revision: TileRevision, handle: H) -> Option<H> {
        self.clock = self.clock.wrapping_add(1);
        let tick = self.clock;
        if let Some(entry) = self.entries.get_mut(&id) {
            entry.revision = revision;
            entry.handle = handle;
            entry.last_used = tick;
            return None;
        }
        if self.capacity == 0 {
            return Some(handle);
        }
        let evicted = if self.entries.len() >= self.capacity {
            self.evict_lru()
        } else {
            None
        };
        self.entries.insert(
            id,
            CacheEntry {
                revision,
                handle,
                last_used: tick,
            },
        );
        if evicted.is_some() {
            self.evicted += 1;
        }
        evicted
    }

    /// Removes a slot and hands its handle back (`None` if absent — the
    /// backend then has nothing to free).
    pub fn invalidate(&mut self, id: TileId) -> Option<H> {
        self.entries.remove(&id).map(|e| e.handle)
    }

    /// Drains the cache and hands all handles back (the caller frees them).
    pub fn invalidate_all(&mut self) -> Vec<H> {
        self.entries
            .drain()
            .map(|(_, entry)| entry.handle)
            .collect()
    }

    /// Evicts the least recently used (internal — the handle bubbles up).
    fn evict_lru(&mut self) -> Option<H> {
        let victim = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(id, _)| *id)?;
        self.entries.remove(&victim).map(|e| e.handle)
    }
}

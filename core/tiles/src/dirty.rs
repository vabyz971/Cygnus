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

//! Dirty set: dense per-level bitset, no `HashMap`.
//!
//! [`DirtyTiles`] tracks tiles to recompute with O(1) marking and
//! lookup, an O(1) counter (`dirty_count`), and deterministic
//! row-major iteration — no allocation after construction (ranges are
//! consumed by value, never collected).

use crate::{TileGrid, TileId, TileRange};

/// Dirty tiles of a surface: one bitset per mip level.
///
/// Sized once for the surface (`DirtyTiles::new`);
/// off-surface markings are ignored (false) — clipping belongs to
/// [`TileGrid::invalidate`], robustness stays here.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DirtyTiles {
    levels: Vec<LevelBits>,
    count: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct LevelBits {
    nx: u32,
    ny: u32,
    words: Vec<u64>,
}

impl DirtyTiles {
    /// New empty set sized for the grid.
    #[must_use]
    pub fn new(grid: &TileGrid) -> Self {
        let mut levels = Vec::with_capacity(usize::from(grid.level_count()));
        for level in 0..grid.level_count() {
            let (nx, ny) = grid.tile_count(level);
            let words = vec![0u64; (u64::from(nx) * u64::from(ny)).div_ceil(64) as usize];
            levels.push(LevelBits { nx, ny, words });
        }
        Self { levels, count: 0 }
    }

    /// Number of tracked levels.
    #[must_use]
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// Number of dirty tiles (O(1) — maintained counter).
    #[must_use]
    pub fn dirty_count(&self) -> u64 {
        self.count
    }

    /// True if no dirty tiles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Marks a tile dirty. Returns true if newly dirty (false if already
    /// dirty or off-surface/level).
    pub fn mark(&mut self, id: TileId) -> bool {
        let Some(bits) = self.levels.get_mut(usize::from(id.level)) else {
            return false;
        };
        if id.x < 0 || id.y < 0 || (id.x as u32) >= bits.nx || (id.y as u32) >= bits.ny {
            return false;
        }
        let index = (id.y as u64) * u64::from(bits.nx) + (id.x as u64);
        let word = (index / 64) as usize;
        let bit = index % 64;
        let mask = 1u64 << bit;
        if bits.words[word] & mask != 0 {
            return false;
        }
        bits.words[word] |= mask;
        self.count += 1;
        true
    }

    /// Marks a range dirty (no allocation — iterated, never collected).
    /// Returns the number of NEWLY dirty tiles.
    pub fn mark_range(&mut self, range: TileRange) -> u64 {
        if range.is_empty() {
            return 0;
        }
        let Some(bits) = self.levels.get_mut(usize::from(range.level)) else {
            return 0;
        };
        // Fast clipping to the level dimensions (rows/columns).
        let nx = bits.nx as i64;
        let ny = bits.ny as i64;
        let x0 = (i64::from(range.x0)).clamp(0, nx - 1);
        let x1 = (i64::from(range.x1)).clamp(0, nx - 1);
        let y0 = (i64::from(range.y0)).clamp(0, ny - 1);
        let y1 = (i64::from(range.y1)).clamp(0, ny - 1);
        if x0 > x1 || y0 > y1 {
            return 0;
        }
        let mut fresh = 0u64;
        for y in y0..=y1 {
            let base = (y as u64) * (nx as u64);
            for x in x0..=x1 {
                let index = base + (x as u64);
                let word = (index / 64) as usize;
                let mask = 1u64 << (index % 64);
                if bits.words[word] & mask == 0 {
                    bits.words[word] |= mask;
                    fresh += 1;
                }
            }
        }
        self.count += fresh;
        fresh
    }

    /// Tests without mutating (false off-surface/level).
    #[must_use]
    pub fn is_dirty(&self, id: TileId) -> bool {
        let Some(bits) = self.levels.get(usize::from(id.level)) else {
            return false;
        };
        if id.x < 0 || id.y < 0 || (id.x as u32) >= bits.nx || (id.y as u32) >= bits.ny {
            return false;
        }
        let index = (id.y as u64) * u64::from(bits.nx) + (id.x as u64);
        bits.words[(index / 64) as usize] & (1u64 << (index % 64)) != 0
    }

    /// Cleans a tile. Returns the PREVIOUS dirty state (false if already
    /// clean or off-surface — the scheduler never sees a cleaned
    /// tile again).
    pub fn clear(&mut self, id: TileId) -> bool {
        let Some(bits) = self.levels.get_mut(usize::from(id.level)) else {
            return false;
        };
        if id.x < 0 || id.y < 0 || (id.x as u32) >= bits.nx || (id.y as u32) >= bits.ny {
            return false;
        }
        let index = (id.y as u64) * u64::from(bits.nx) + (id.x as u64);
        let word = (index / 64) as usize;
        let mask = 1u64 << (index % 64);
        if bits.words[word] & mask == 0 {
            return false;
        }
        bits.words[word] &= !mask;
        self.count -= 1;
        true
    }

    /// Cleans everything (end of a full pass).
    pub fn clear_all(&mut self) {
        for bits in &mut self.levels {
            bits.words.fill(0);
        }
        self.count = 0;
    }

    /// Iterates dirty tiles row-major, level by level (no allocation).
    #[must_use]
    pub fn iter_dirty(&self) -> DirtyIter<'_> {
        DirtyIter {
            dirty: self,
            level: 0,
            word: 0,
            bit: 0,
        }
    }
}

/// Dirty-tile iterator (rows, ascending levels, no allocation).
#[derive(Debug)]
pub struct DirtyIter<'a> {
    dirty: &'a DirtyTiles,
    level: usize,
    word: usize,
    bit: u32,
}

impl Iterator for DirtyIter<'_> {
    type Item = TileId;

    fn next(&mut self) -> Option<TileId> {
        loop {
            let bits = self.dirty.levels.get(self.level)?;
            if self.word >= bits.words.len() {
                self.level += 1;
                self.word = 0;
                self.bit = 0;
                continue;
            }
            let mut word = bits.words[self.word] >> self.bit;
            if word == 0 {
                self.word += 1;
                self.bit = 0;
                continue;
            }
            let zeros = word.trailing_zeros();
            self.bit += zeros;
            word >>= zeros;
            debug_assert!(word & 1 == 1);
            let index = self.word as u64 * 64 + u64::from(self.bit);
            self.bit += 1;
            if self.bit >= 64 {
                self.word += 1;
                self.bit = 0;
            }
            let nx = u64::from(bits.nx);
            let x = (index % nx) as i32;
            let y = (index / nx) as i32;
            if (x as u32) < bits.nx && (y as u32) < bits.ny {
                return Some(TileId::new(self.level as u8, x, y));
            }
        }
    }
}

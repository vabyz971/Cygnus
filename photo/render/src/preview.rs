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

//! Reduced sources (docs/decisions/0024, point 3, and 0034, T3): a tile of
//! pyramid level L of a layer or a mask, the average of the 2^L × 2^L
//! source tiles it covers. They feed the approximate tiles the canvas
//! shows first (a coarse first view, the preview of a drag); the exact
//! tiles always come from the full-size sources.
//!
//! They are computed on the processor, on worker threads, from the tiles of
//! the document: the GPU only receives reduced tiles. Layers are averaged
//! in linear light with premultiplied alpha (docs/decisions/0021), masks as
//! plain coverage. A reduced tile remembers the source tiles it was built
//! from (weak references, so that an allocation cannot be reused while the
//! entry lives): when one of them changes, the entry is stale and is built
//! again on demand. The cache is bounded to a tenth of the pixel memory of
//! the document and evicts the least recently used entries first.

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};

use cygnus_photo_core::pixels::{TILE_SIZE, Tile, TileCoord};
use cygnus_photo_core::{Depth, Document, NodeId, NodeKind};

use crate::program::layer_tile;
use crate::sources::SourceKey;

/// Coarsest level composed from reduced sources: a reduced tile reads at
/// most 16 × 16 source tiles.
pub const MAX_PREVIEW_LEVEL: u32 = 4;
/// Smallest memory budget of the reduced sources, for small documents.
pub const MIN_REDUCED_BUDGET: u64 = 32 * 1024 * 1024;
/// Bytes of one reduced layer tile (RGBA, 16 bits); a mask tile is a
/// quarter.
pub(crate) const REDUCED_TILE_BYTES: u64 = (TILE_SIZE * TILE_SIZE * 8) as u64;
/// Worker threads at most.
const MAX_WORKERS: usize = 4;
/// Entries of the encoding table, over the square root of linear values.
const ENCODE_STEPS: usize = 4096;

/// Memory budget of the reduced sources of `document`: a tenth of its pixel
/// memory (docs/decisions/0024), at least [`MIN_REDUCED_BUDGET`].
pub fn reduced_budget(document: &Document) -> u64 {
    let pixels: u64 = document
        .pixels
        .layers()
        .flat_map(|layer| document.pixels.tiles(layer))
        .map(|(_, tile)| tile.bytes() as u64)
        .sum::<u64>()
        + document
            .sources
            .values()
            .flat_map(|source| source.tiles.values())
            .map(|tile| tile.bytes() as u64)
            .sum::<u64>();
    (pixels / 10).max(MIN_REDUCED_BUDGET)
}

/// sRGB transfer function (IEC 61966-2-1), encoded value to linear light.
/// The processor copy of `srgb_to_linear` in color.wgsl, used only to
/// reduce sources.
fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Inverse sRGB transfer function, linear light to encoded value.
fn linear_to_srgb(c: f32) -> f32 {
    let v = c.clamp(0.0, 1.0);
    if v <= 0.003_130_8 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}

/// Linear value of every 16-bit encoded value.
fn decode_table() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=u16::MAX)
            .map(|v| srgb_to_linear(f32::from(v) / 65535.0))
            .collect()
    })
}

/// Encoded value of linear values sampled at the squares of a uniform grid:
/// the curve is smooth in the square root, so that linear interpolation
/// stays within 1e-6 of the formula.
fn encode_table() -> &'static [f32] {
    static TABLE: OnceLock<Vec<f32>> = OnceLock::new();
    TABLE.get_or_init(|| {
        (0..=ENCODE_STEPS)
            .map(|i| {
                let t = i as f32 / ENCODE_STEPS as f32;
                linear_to_srgb(t * t)
            })
            .collect()
    })
}

/// Encoded value of the linear value `linear`, from [`encode_table`].
fn encode(linear: f32) -> f32 {
    let table = encode_table();
    let position = linear.clamp(0.0, 1.0).sqrt() * ENCODE_STEPS as f32;
    let index = (position as usize).min(ENCODE_STEPS - 1);
    let fraction = position - index as f32;
    table[index] + (table[index + 1] - table[index]) * fraction
}

/// Value `i` of a tile, as 16 bits.
fn value16(tile: &Tile, i: usize) -> u16 {
    match tile {
        Tile::U8(values) | Tile::Gray8(values) => u16::from(values[i]) * 257,
        Tile::U16(values) | Tile::Gray16(values) => values[i],
    }
}

/// The reduced layer tile of `children`: `factor` × `factor` source tiles,
/// row by row, `None` where the layer has no tile (transparent). Each pixel
/// is the average of `factor` × `factor` source pixels in linear light with
/// premultiplied alpha, stored as straight 16-bit sRGB values.
pub(crate) fn reduce_layer(children: &[Option<Arc<Tile>>], factor: u32) -> Tile {
    let decode = decode_table();
    let side = TILE_SIZE as usize;
    let f = factor as usize;
    let count = (f * f) as f32;
    let mut out = vec![0u16; side * side * 4];
    for y in 0..side {
        for x in 0..side {
            let mut sum = [0f32; 4];
            for dy in 0..f {
                let sy = y * f + dy;
                for dx in 0..f {
                    let sx = x * f + dx;
                    let Some(tile) = &children[(sy / side) * f + sx / side] else {
                        continue;
                    };
                    let i = ((sy % side) * side + sx % side) * 4;
                    let alpha = f32::from(value16(tile, i + 3)) / 65535.0;
                    if alpha > 0.0 {
                        for (c, total) in sum.iter_mut().take(3).enumerate() {
                            *total += decode[usize::from(value16(tile, i + c))] * alpha;
                        }
                        sum[3] += alpha;
                    }
                }
            }
            let o = (y * side + x) * 4;
            if sum[3] > 0.0 {
                for c in 0..3 {
                    let straight = sum[c] / sum[3];
                    out[o + c] = (encode(straight) * 65535.0).round() as u16;
                }
                out[o + 3] = ((sum[3] / count).min(1.0) * 65535.0).round() as u16;
            }
        }
    }
    Tile::U16(out.into_boxed_slice())
}

/// The reduced mask tile of `children` (as [`reduce_layer`]): the average
/// coverage, `empty` (16 bits) where the mask has no tile.
pub(crate) fn reduce_mask(children: &[Option<Arc<Tile>>], factor: u32, empty: u16) -> Tile {
    let side = TILE_SIZE as usize;
    let f = factor as usize;
    let count = (f * f) as u64;
    let mut out = vec![0u16; side * side];
    for y in 0..side {
        for x in 0..side {
            let mut sum = 0u64;
            for dy in 0..f {
                let sy = y * f + dy;
                for dx in 0..f {
                    let sx = x * f + dx;
                    sum += u64::from(match &children[(sy / side) * f + sx / side] {
                        Some(tile) => value16(tile, (sy % side) * side + sx % side),
                        None => empty,
                    });
                }
            }
            out[y * side + x] = ((sum + count / 2) / count) as u16;
        }
    }
    Tile::Gray16(out.into_boxed_slice())
}

/// What a reduced tile was built from: the source tiles it covers.
type Stamp = Vec<Option<Weak<Tile>>>;

fn same(stamp: &Stamp, children: &[Option<&Arc<Tile>>]) -> bool {
    stamp.len() == children.len()
        && stamp
            .iter()
            .zip(children)
            .all(|(old, new)| match (old, new) {
                (None, None) => true,
                (Some(old), Some(new)) => std::ptr::eq(old.as_ptr(), Arc::as_ptr(new)),
                _ => false,
            })
}

/// The source tiles covered by `key` at `level`, row by row.
fn children(document: &Document, key: SourceKey, level: u32) -> Vec<Option<&Arc<Tile>>> {
    let factor = 1u32 << level;
    let mut out = Vec::with_capacity((factor * factor) as usize);
    for dy in 0..factor {
        for dx in 0..factor {
            let coord = TileCoord {
                tx: key.1.tx * factor + dx,
                ty: key.1.ty * factor + dy,
            };
            out.push(layer_tile(document, key.0, coord));
        }
    }
    out
}

/// Empty value of the mask `id` in 16 bits, or `None` for a layer.
fn mask_empty(document: &Document, id: NodeId) -> Option<u16> {
    match document.node(id).map(|node| &node.kind) {
        Some(NodeKind::Mask { empty, .. }) => Some(match document.depth {
            Depth::U8 => empty.saturating_mul(257),
            Depth::U16 => *empty,
        }),
        _ => None,
    }
}

/// One reduction for a worker.
struct Job {
    key: (SourceKey, u32),
    children: Vec<Option<Arc<Tile>>>,
    mask_empty: Option<u16>,
}

/// A reduction done.
struct Done {
    key: (SourceKey, u32),
    stamp: Stamp,
    tile: Arc<Tile>,
}

struct Entry {
    stamp: Stamp,
    tile: Arc<Tile>,
    used: u64,
}

/// Counters of the reduced sources.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ReducedStats {
    /// Reduced tiles computed.
    pub(crate) built: u64,
    /// Bytes held now.
    pub(crate) bytes: u64,
}

/// Reduced tiles computed so far, and the workers computing the others.
pub(crate) struct ReducedCache {
    entries: HashMap<(SourceKey, u32), Entry>,
    /// Requested reductions, with what they are built from.
    pending: HashSet<(SourceKey, u32)>,
    jobs: Option<Sender<Job>>,
    results: Receiver<Done>,
    budget: u64,
    frame: u64,
    pub(crate) stats: ReducedStats,
}

impl ReducedCache {
    pub(crate) fn new() -> Self {
        let (jobs, inbox) = channel::<Job>();
        let (outbox, results) = channel::<Done>();
        let inbox = Arc::new(Mutex::new(inbox));
        let workers = std::thread::available_parallelism()
            .map_or(1, |n| n.get().saturating_sub(1))
            .clamp(1, MAX_WORKERS);
        for index in 0..workers {
            let inbox = Arc::clone(&inbox);
            let outbox = outbox.clone();
            // A worker that cannot start leaves the others; with none, the
            // reductions are computed when waited for.
            let _ = std::thread::Builder::new()
                .name(format!("cygnus reduce {index}"))
                .spawn(move || worker(&inbox, &outbox));
        }
        Self {
            entries: HashMap::new(),
            pending: HashSet::new(),
            jobs: Some(jobs),
            results,
            budget: MIN_REDUCED_BUDGET,
            frame: 0,
            stats: ReducedStats::default(),
        }
    }

    /// Starts a frame: takes in the finished reductions and sets the budget
    /// for `document`. Entries used during a frame are never evicted in it.
    pub(crate) fn begin_frame(&mut self, document: &Document) {
        self.frame += 1;
        self.budget = reduced_budget(document);
        while let Ok(done) = self.results.try_recv() {
            self.insert(done);
        }
    }

    pub(crate) fn budget(&self) -> u64 {
        self.budget
    }

    fn insert(&mut self, done: Done) {
        self.pending.remove(&done.key);
        self.stats.built += 1;
        self.stats.bytes += done.tile.bytes() as u64;
        if let Some(old) = self.entries.insert(
            done.key,
            Entry {
                stamp: done.stamp,
                tile: done.tile,
                used: self.frame,
            },
        ) {
            self.stats.bytes -= old.tile.bytes() as u64;
        }
        // Least recently used first, never what this frame uses.
        while self.stats.bytes > self.budget {
            let victim = self
                .entries
                .iter()
                .filter(|(_, entry)| entry.used < self.frame)
                .min_by_key(|(key, entry)| (entry.used, key.1, key.0))
                .map(|(key, _)| *key);
            let Some(victim) = victim else {
                break;
            };
            if let Some(entry) = self.entries.remove(&victim) {
                self.stats.bytes -= entry.tile.bytes() as u64;
            }
        }
    }

    /// The current reduced tile of `key` at `level`, if computed; marks it
    /// used by this frame.
    pub(crate) fn get(
        &mut self,
        document: &Document,
        key: SourceKey,
        level: u32,
    ) -> Option<Arc<Tile>> {
        let children = children(document, key, level);
        let entry = self.entries.get_mut(&(key, level))?;
        if !same(&entry.stamp, &children) {
            return None;
        }
        entry.used = self.frame;
        Some(Arc::clone(&entry.tile))
    }

    /// Whether every reduced tile of `keys` at `level` is current; requests
    /// the missing ones. With `wait`, waits for them (or computes them here
    /// if no worker runs).
    pub(crate) fn ready_or_wait(
        &mut self,
        document: &Document,
        keys: &[SourceKey],
        level: u32,
        wait: bool,
    ) -> bool {
        let mut missing = Vec::new();
        for key in keys {
            if self.get(document, *key, level).is_none() {
                missing.push(*key);
            }
        }
        if missing.is_empty() {
            return true;
        }
        for key in &missing {
            let entry_key = (*key, level);
            if self.pending.contains(&entry_key) {
                continue;
            }
            let job = Job {
                key: entry_key,
                children: children(document, *key, level)
                    .into_iter()
                    .map(|tile| tile.map(Arc::clone))
                    .collect(),
                mask_empty: mask_empty(document, key.0),
            };
            match self.jobs.as_ref().map(|jobs| jobs.send(job)) {
                Some(Ok(())) => {
                    self.pending.insert(entry_key);
                }
                // No worker: computed here.
                Some(Err(error)) => {
                    let done = run(error.0);
                    self.insert(done);
                }
                None => {}
            }
        }
        if !wait {
            return false;
        }
        while missing
            .iter()
            .any(|key| self.pending.contains(&(*key, level)))
        {
            match self.results.recv() {
                Ok(done) => self.insert(done),
                Err(_) => break,
            }
        }
        missing
            .iter()
            .all(|key| self.get(document, *key, level).is_some())
    }
}

impl Drop for ReducedCache {
    fn drop(&mut self) {
        // Closing the job channel stops the workers.
        self.jobs = None;
    }
}

fn run(job: Job) -> Done {
    let factor = 1u32 << job.key.1;
    let stamp = job
        .children
        .iter()
        .map(|c| c.as_ref().map(Arc::downgrade))
        .collect();
    let tile = match job.mask_empty {
        Some(empty) => reduce_mask(&job.children, factor, empty),
        None => reduce_layer(&job.children, factor),
    };
    Done {
        key: job.key,
        stamp,
        tile: Arc::new(tile),
    }
}

fn worker(inbox: &Mutex<Receiver<Job>>, outbox: &Sender<Done>) {
    loop {
        let job = {
            let inbox = inbox.lock().unwrap_or_else(PoisonError::into_inner);
            inbox.recv()
        };
        let Ok(job) = job else {
            return;
        };
        if outbox.send(run(job)).is_err() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIDE: usize = TILE_SIZE as usize;

    fn rgba16(pixel: impl Fn(usize, usize) -> [u16; 4]) -> Arc<Tile> {
        let mut values = Vec::with_capacity(SIDE * SIDE * 4);
        for y in 0..SIDE {
            for x in 0..SIDE {
                values.extend(pixel(x, y));
            }
        }
        Arc::new(Tile::U16(values.into_boxed_slice()))
    }

    fn get(tile: &Tile, x: usize, y: usize) -> [u16; 4] {
        tile.get(x as u32, y as u32)
    }

    /// The reduced value computed by hand, in f64: linear light,
    /// premultiplied.
    fn by_hand(pixels: &[[u16; 4]]) -> [u16; 4] {
        let decode = |v: u16| {
            let c = f64::from(v) / 65535.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        let encode = |v: f64| {
            if v <= 0.003_130_8 {
                v * 12.92
            } else {
                1.055 * v.powf(1.0 / 2.4) - 0.055
            }
        };
        let alpha: f64 = pixels.iter().map(|p| f64::from(p[3]) / 65535.0).sum();
        let mut out = [0u16; 4];
        for c in 0..3 {
            let sum: f64 = pixels
                .iter()
                .map(|p| decode(p[c]) * f64::from(p[3]) / 65535.0)
                .sum();
            out[c] = (encode(sum / alpha) * 65535.0).round() as u16;
        }
        out[3] = (alpha / pixels.len() as f64 * 65535.0).round() as u16;
        out
    }

    #[test]
    fn layers_are_averaged_in_linear_light_with_premultiplied_alpha() {
        // A checker of two translucent colors: each reduced pixel averages
        // two of each.
        let a = [60000, 3000, 20000, 65535];
        let b = [1000, 50000, 40000, 30000];
        let tile = rgba16(|x, y| if (x + y) % 2 == 0 { a } else { b });
        let reduced = reduce_layer(&[Some(Arc::clone(&tile)), None, None, None], 2);
        let expected = by_hand(&[a, b, b, a]);
        for (x, y) in [(0, 0), (37, 90), (127, 127)] {
            let got = get(&reduced, x, y);
            for c in 0..4 {
                assert!(
                    got[c].abs_diff(expected[c]) <= 1,
                    "({x}, {y}) channel {c}: {} against {}",
                    got[c],
                    expected[c]
                );
            }
        }
        // Transparent black and opaque white: premultiplied, the color
        // stays white, the alpha halves.
        let tile = rgba16(|x, _| if x % 2 == 0 { [0; 4] } else { [65535; 4] });
        let reduced = reduce_layer(&[Some(tile), None, None, None], 2);
        assert_eq!(get(&reduced, 10, 10), [65535, 65535, 65535, 32768]);
    }

    #[test]
    fn opaque_black_and_white_average_to_the_linear_middle() {
        let tile = rgba16(|x, _| {
            if x % 2 == 0 {
                [0, 0, 0, 65535]
            } else {
                [65535; 4]
            }
        });
        let reduced = reduce_layer(&[Some(tile), None, None, None], 2);
        // Linear 0.5 encodes to 0.7354, not 0.5.
        let expected = (linear_to_srgb(0.5) * 65535.0).round() as u16;
        let value = get(&reduced, 3, 4);
        assert!(value[0].abs_diff(expected) <= 1, "{value:?}");
        assert_eq!(value[3], 65535);
    }

    #[test]
    fn missing_children_are_transparent_and_cover_their_area() {
        let opaque = rgba16(|_, _| [65535, 0, 0, 65535]);
        // Factor 2: the top left quarter of the reduced tile comes from the
        // first child, the rest from missing ones.
        let reduced = reduce_layer(&[Some(opaque), None, None, None], 2);
        assert_eq!(get(&reduced, 10, 10), [65535, 0, 0, 65535]);
        assert_eq!(get(&reduced, 200, 10), [0, 0, 0, 0]);
        assert_eq!(get(&reduced, 10, 200), [0, 0, 0, 0]);
    }

    #[test]
    fn masks_average_their_coverage_and_use_the_empty_value() {
        let values: Vec<u16> = (0..SIDE * SIDE)
            .map(|i| if i % 2 == 0 { 0 } else { 60000 })
            .collect();
        let mask = Arc::new(Tile::Gray16(values.into_boxed_slice()));
        let reduced = reduce_mask(&[Some(mask), None, None, None], 2, 65535);
        assert_eq!(get(&reduced, 0, 0)[0], 30000);
        assert_eq!(get(&reduced, 200, 200)[0], 65535);
    }

    #[test]
    fn eight_bit_sources_are_widened_exactly() {
        let tile = Arc::new(Tile::U8(vec![255u8; SIDE * SIDE * 4].into_boxed_slice()));
        let reduced = reduce_layer(&[Some(tile), None, None, None], 2);
        assert_eq!(get(&reduced, 0, 0), [65535; 4]);
    }

    #[test]
    fn the_encoding_table_follows_the_formula() {
        let mut worst = 0f32;
        for i in 0..=100_000 {
            let x = i as f32 / 100_000.0;
            worst = worst.max((encode(x) - linear_to_srgb(x)).abs());
        }
        assert!(worst * 65535.0 < 0.1, "{} units", worst * 65535.0);
    }

    fn done(layer: u64, tx: u32) -> Done {
        Done {
            key: ((NodeId(layer), TileCoord { tx, ty: 0 }), 1),
            stamp: Vec::new(),
            tile: rgba16(|_, _| [0; 4]),
        }
    }

    #[test]
    fn the_cache_keeps_its_budget_evicting_the_oldest_first() {
        let mut cache = ReducedCache::new();
        cache.budget = 2 * REDUCED_TILE_BYTES;
        cache.frame = 1;
        cache.insert(done(1, 0));
        cache.frame = 2;
        cache.insert(done(1, 1));
        cache.frame = 3;
        cache.insert(done(1, 2));
        assert_eq!(cache.stats.bytes, 2 * REDUCED_TILE_BYTES);
        assert_eq!(cache.entries.len(), 2);
        assert!(
            !cache
                .entries
                .contains_key(&((NodeId(1), TileCoord { tx: 0, ty: 0 }), 1)),
            "the oldest entry was kept"
        );
        // What the current frame uses is never evicted, even over budget.
        cache.insert(done(1, 3));
        cache.insert(done(1, 4));
        assert_eq!(cache.entries.len(), 3);
    }

    #[test]
    fn the_budget_is_a_tenth_of_the_pixels_with_a_floor() {
        assert_eq!(reduced_budget(&Document::default()), MIN_REDUCED_BUDGET);
    }

    #[test]
    fn stamps_detect_a_changed_source_tile() {
        let a = rgba16(|_, _| [1, 2, 3, 4]);
        let b = rgba16(|_, _| [1, 2, 3, 4]);
        let stamp: Stamp = vec![Some(Arc::downgrade(&a)), None];
        assert!(same(&stamp, &[Some(&a), None]));
        assert!(!same(&stamp, &[Some(&b), None]));
        assert!(!same(&stamp, &[None, None]));
    }
}

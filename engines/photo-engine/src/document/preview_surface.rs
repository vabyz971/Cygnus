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

//! Persistent preview surface (Phase 6G): keep the produced image, only
//! write the needed regions.
//!
//! [`PreviewSurface`] holds ONE RGBA8 buffer (render perimeter dimensions)
//! surviving across frames. The worker blits newly rendered tiles into
//! it ([`PreviewSurface::update_tile`]); clean zones are never
//! rewritten. Strict role (6G rule §1): the surface knows neither dirty
//! nor scope — it writes what it is given, bounded to its dimensions
//! (no screen coordinates, all in document/buffer-space).
//!
//! Explicit full rebuild ONLY on: first initialization, perimeter
//! change ([`PreviewSurface::ensure`]), `clear` + repopulation, reset.
//! Everywhere else: local updates.
//!
//! Accepted trade-off (§25 6G): `TileCache` hits are copied to the
//! surface (no shared `Arc` yet) — 6G removes useless rewrites,
//! not all copies.

use image::{DynamicImage, ImageBuffer, Rgba};

use super::tree::RegionalComposite;

/// Surface statistics (§7 6G + §6 6G.1): counters and timers, cumulative,
/// no render effect.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreviewSurfaceStats {
    /// Buffer (re)allocations (init, resize, explicit reset).
    pub full_updates: u64,
    /// `update_tiles` calls having written at least one tile.
    pub partial_updates: u64,
    /// Tiles applied in total.
    pub tiles_applied: u64,
    /// Pixels written in total (blits, excluding crop reads).
    pub pixels_written: u64,
    /// Cumulative (re)allocation time (µs, Phase 6G.1).
    pub ensure_us: u128,
    /// Cumulative `clear` time (µs, Phase 6G.1).
    pub clear_us: u128,
    /// Cumulative `update_tiles` time (µs, Phase 6G.1).
    pub update_us: u128,
}

/// Persistent preview image (buffer + dimensions + counters).
#[derive(Debug, Clone)]
pub struct PreviewSurface {
    buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
    stats: PreviewSurfaceStats,
}

impl PreviewSurface {
    /// New empty surface (`width`/`height` clamped to ≥ 1). Counts one
    /// `full_updates` (explicit initial allocation).
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let mut surface = Self {
            buf: ImageBuffer::from_pixel(1, 1, Rgba([0, 0, 0, 0])),
            stats: PreviewSurfaceStats::default(),
        };
        surface.ensure(width, height);
        surface
    }

    /// Current dimensions.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.buf.width(), self.buf.height())
    }

    /// True if empty (never after `new`, only internal transient).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.width() == 0 || self.buf.height() == 0
    }

    /// Ensures dimensions: reallocates (zeros + `full_updates`) if
    /// different, else touches nothing. Returns true if recreated. The
    /// only full-rebuild path with `clear`.
    pub fn ensure(&mut self, width: u32, height: u32) -> bool {
        // 6G.1 instrumentation: allocation timer (no effect).
        let t = std::time::Instant::now();
        let (width, height) = (width.max(1), height.max(1));
        if self.buf.width() == width && self.buf.height() == height {
            return false;
        }
        self.buf = ImageBuffer::from_pixel(width, height, Rgba([0, 0, 0, 0]));
        self.stats.full_updates += 1;
        self.stats.ensure_us += t.elapsed().as_micros();
        true
    }

    /// Clears the content (zeros, dimensions kept): starting point for a
    /// repopulation from cache, without reallocation.
    pub fn clear(&mut self) {
        // 6G.1 instrumentation (no effect).
        let t = std::time::Instant::now();
        for px in self.buf.pixels_mut() {
            *px = Rgba([0, 0, 0, 0]);
        }
        self.stats.clear_us += t.elapsed().as_micros();
    }

    /// Writes a tile at its buffer origin, clipped to the surface. Out-of-
    /// surface fringes are ignored (never a screen offset).
    pub fn update_tile(&mut self, tile: &RegionalComposite) {
        self.update_tiles(std::slice::from_ref(tile));
    }

    /// Writes several tiles; counts ONE `partial_updates` if at least one
    /// tile wrote ≥ 1 pixel (empty or fully out-of-view call: nothing).
    pub fn update_tiles(&mut self, tiles: &[RegionalComposite]) {
        // 6G.1 instrumentation (no effect).
        let t = std::time::Instant::now();
        let mut wrote = false;
        for tile in tiles {
            wrote |= self.blit(tile);
        }
        if wrote {
            self.stats.partial_updates += 1;
        }
        self.stats.update_us += t.elapsed().as_micros();
    }

    /// Blits a tile; true if ≥ 1 pixel written (`pixels_written` counts the
    /// exact count (clipped fringes excluded), not the tile area).
    fn blit(&mut self, tile: &RegionalComposite) -> bool {
        let buf = match tile.image.as_rgba8() {
            Some(b) => b.clone(),
            None => tile.image.to_rgba8(),
        };
        let (sw, sh) = (self.buf.width() as i64, self.buf.height() as i64);
        let mut written = 0u64;
        for (x, y, px) in buf.enumerate_pixels() {
            let dx = i64::from(tile.origin_x) + i64::from(x);
            let dy = i64::from(tile.origin_y) + i64::from(y);
            if dx >= 0 && dy >= 0 && dx < sw && dy < sh {
                self.buf.put_pixel(dx as u32, dy as u32, *px);
                written += 1;
            }
        }
        if written > 0 {
            self.stats.tiles_applied += 1;
            self.stats.pixels_written += written;
        }
        written > 0
    }

    /// Surface view (borrow, zero copy).
    #[must_use]
    pub fn image(&self) -> &ImageBuffer<Rgba<u8>, Vec<u8>> {
        &self.buf
    }

    /// Owned image (crop or clone for presentation).
    #[must_use]
    pub fn to_image(&self) -> DynamicImage {
        DynamicImage::ImageRgba8(self.buf.clone())
    }

    /// Cumulative counters.
    #[must_use]
    pub fn stats(&self) -> PreviewSurfaceStats {
        self.stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tile(x: u32, y: u32, w: u32, h: u32, v: u8) -> RegionalComposite {
        RegionalComposite {
            image: DynamicImage::ImageRgba8(ImageBuffer::from_pixel(w, h, Rgba([v, v, v, 255]))),
            origin_x: x,
            origin_y: y,
        }
    }

    #[test]
    fn init_pleine_puis_partiel() {
        let mut s = PreviewSurface::new(512, 512);
        assert_eq!(s.dimensions(), (512, 512));
        assert_eq!(s.stats().full_updates, 1);
        // Initial fill: four 256² tiles = 262144 px written.
        s.update_tiles(&[
            tile(0, 0, 256, 256, 10),
            tile(256, 0, 256, 256, 20),
            tile(0, 256, 256, 256, 30),
            tile(256, 256, 256, 256, 40),
        ]);
        assert_eq!(s.stats().partial_updates, 1);
        assert_eq!(s.stats().tiles_applied, 4);
        assert_eq!(s.stats().pixels_written, 512 * 512);
        assert_eq!(s.image().get_pixel(0, 0)[0], 10);
        assert_eq!(s.image().get_pixel(511, 511)[0], 40);
        // Partial update: 1 tile, the rest intact.
        let before = s.image().clone();
        s.update_tile(&tile(0, 0, 256, 256, 99));
        assert_eq!(s.stats().partial_updates, 2);
        assert_eq!(s.stats().tiles_applied, 5);
        assert_eq!(s.stats().pixels_written, 512 * 512 + 256 * 256);
        assert_eq!(s.image().get_pixel(0, 0)[0], 99);
        assert_eq!(s.image().get_pixel(511, 511)[0], 40);
        // Outside the overlap with `before`: unchanged.
        for (a, b) in before.enumerate_pixels().zip(s.image().enumerate_pixels()) {
            let ((x, _, _), (_, _, _)) = (a, b);
            if x >= 256 {
                assert_eq!(a.2, b.2, "hors tuile : inchangé en x={x}");
            }
        }
    }

    #[test]
    fn appel_vide_et_hors_champ() {
        let mut s = PreviewSurface::new(64, 64);
        let n = s.stats();
        s.update_tiles(&[]);
        let after_empty = s.stats();
        assert_eq!(after_empty.tiles_applied, n.tiles_applied);
        assert_eq!(after_empty.pixels_written, n.pixels_written);
        assert_eq!(after_empty.partial_updates, n.partial_updates);
        s.update_tile(&tile(500, 500, 10, 10, 7));
        let after_oob = s.stats();
        assert_eq!(
            after_oob.tiles_applied, n.tiles_applied,
            "hors champ : rien"
        );
        assert_eq!(after_oob.pixels_written, n.pixels_written);
        // 6G.1 note: timers themselves advance on every call (even empty) —
        // measuring has a cost, event counters do not.
    }

    #[test]
    fn ensure_resize_et_clear() {
        let mut s = PreviewSurface::new(64, 64);
        assert!(!s.ensure(64, 64), "mêmes dims : rien");
        assert_eq!(s.stats().full_updates, 1);
        assert!(s.ensure(128, 64), "resize : recréée");
        assert_eq!(s.dimensions(), (128, 64));
        assert_eq!(s.stats().full_updates, 2);
        s.update_tile(&tile(0, 0, 128, 64, 5));
        s.clear();
        assert_eq!(s.dimensions(), (128, 64), "clear garde les dims");
        assert_eq!(s.stats().full_updates, 2, "clear ≠ rebuild");
        assert!(s.image().pixels().all(|p| p.0 == [0, 0, 0, 0]));
    }

    #[test]
    fn tuile_partielle_bord() {
        // Tile overflowing right/bottom: only pixels inside are written AND
        // counted (10×10, not 256²).
        let mut s = PreviewSurface::new(100, 100);
        s.update_tile(&tile(90, 90, 256, 256, 7));
        assert_eq!(s.image().get_pixel(99, 99)[0], 7);
        assert_eq!(s.image().get_pixel(0, 0)[0], 0);
        assert_eq!(s.stats().tiles_applied, 1);
        assert_eq!(s.stats().pixels_written, 100);
    }
}

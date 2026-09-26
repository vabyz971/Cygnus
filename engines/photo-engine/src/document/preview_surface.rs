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

//! Surface de preview persistante (Phase 6G) : conserver l'image produite,
//! n'écrire que les régions nécessaires.
//!
//! [`PreviewSurface`] détient UN buffer RGBA8 (dimensions du périmètre de
//! rendu) qui survit d'une frame à l'autre. Le worker y blitte les tuiles
//! nouvellement rendues ([`PreviewSurface::update_tile`]) ; les zones
//! propres ne sont jamais réécrites. Rôle strict (règle 6G §1) : la surface
//! ne connaît ni dirty ni scope — elle écrit ce qu'on lui donne, borné à
//! ses dimensions (aucune coordonnée écran, tout en document/buffer-space).
//!
//! Reconstruction complète explicite UNIQUEMENT sur : première
//! initialisation, changement de périmètre ([`PreviewSurface::ensure`]),
//! `clear` + repeuplement, reset. Partout ailleurs : mises à jour locales.
//!
//! Compromis assumé (§25 6G) : les hits du `TileCache` sont recopiés vers la
//! surface (pas encore d'`Arc` partagé) — 6G élimine la réécriture inutile,
//! pas toutes les copies.

use image::{DynamicImage, ImageBuffer, Rgba};

use super::tree::RegionalComposite;

/// Statistiques de la surface (§7 6G + §6 6G.1) : compteurs et chronos
/// cumulés, sans effet sur le rendu.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PreviewSurfaceStats {
    /// (Ré)allocations du buffer (init, resize, reset explicite).
    pub full_updates: u64,
    /// Appels `update_tiles` ayant écrit au moins une tuile.
    pub partial_updates: u64,
    /// Tuiles appliquées au total.
    pub tiles_applied: u64,
    /// Pixels écrits au total (blits, hors lecture de découpe).
    pub pixels_written: u64,
    /// Temps cumulé des (ré)allocations (µs, Phase 6G.1).
    pub ensure_us: u128,
    /// Temps cumulé des `clear` (µs, Phase 6G.1).
    pub clear_us: u128,
    /// Temps cumulé des `update_tiles` (µs, Phase 6G.1).
    pub update_us: u128,
}

/// Image de preview persistante (buffer + dimensions + compteurs).
#[derive(Debug, Clone)]
pub struct PreviewSurface {
    buf: ImageBuffer<Rgba<u8>, Vec<u8>>,
    stats: PreviewSurfaceStats,
}

impl PreviewSurface {
    /// Nouvelle surface vide (`width`/`height` ramenées à ≥ 1).
    /// Compte une `full_updates` (allocation initiale explicite).
    #[must_use]
    pub fn new(width: u32, height: u32) -> Self {
        let mut surface = Self {
            buf: ImageBuffer::from_pixel(1, 1, Rgba([0, 0, 0, 0])),
            stats: PreviewSurfaceStats::default(),
        };
        surface.ensure(width, height);
        surface
    }

    /// Dimensions actuelles.
    #[must_use]
    pub fn dimensions(&self) -> (u32, u32) {
        (self.buf.width(), self.buf.height())
    }

    /// Vrai si vide (jamais après `new`, que du transitoire interne).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.width() == 0 || self.buf.height() == 0
    }

    /// Garantit les dimensions : réalloue (zéros + `full_updates`) si
    /// différentes, sinon ne touche à rien. Retourne vrai si recréée.
    /// Seul chemin de reconstruction complète avec `clear`.
    pub fn ensure(&mut self, width: u32, height: u32) -> bool {
        // Instrumentation 6G.1 : chrono d'allocation (aucun effet).
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

    /// Efface le contenu (zéros, dimensions conservées) : point de départ
    /// d'un repeuplement depuis le cache, sans réallocation.
    pub fn clear(&mut self) {
        // Instrumentation 6G.1 (aucun effet).
        let t = std::time::Instant::now();
        for px in self.buf.pixels_mut() {
            *px = Rgba([0, 0, 0, 0]);
        }
        self.stats.clear_us += t.elapsed().as_micros();
    }

    /// Écrit une tuile à son origine buffer, clippée à la surface.
    /// Les franges hors surface sont ignorées (jamais d'offset écran).
    pub fn update_tile(&mut self, tile: &RegionalComposite) {
        self.update_tiles(std::slice::from_ref(tile));
    }

    /// Écrit plusieurs tuiles ; compte UN `partial_updates` si au moins une
    /// tuile a écrit ≥ 1 pixel (appel vide ou totalement hors champ : rien).
    pub fn update_tiles(&mut self, tiles: &[RegionalComposite]) {
        // Instrumentation 6G.1 (aucun effet).
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

    /// Blit une tuile ; vrai si ≥ 1 pixel écrit (`pixels_written` compte
    /// l'exact (franges clippées exclues), pas l'aire de la tuile).
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

    /// Vue de la surface (emprunt, zéro copie).
    #[must_use]
    pub fn image(&self) -> &ImageBuffer<Rgba<u8>, Vec<u8>> {
        &self.buf
    }

    /// Image possédée (découpe ou clone pour la présentation).
    #[must_use]
    pub fn to_image(&self) -> DynamicImage {
        DynamicImage::ImageRgba8(self.buf.clone())
    }

    /// Compteurs cumulés.
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
        // Remplissage initial : 4 tuiles 256² = 262144 px écrits.
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
        // Mise à jour partielle : 1 tuile, le reste intact.
        let before = s.image().clone();
        s.update_tile(&tile(0, 0, 256, 256, 99));
        assert_eq!(s.stats().partial_updates, 2);
        assert_eq!(s.stats().tiles_applied, 5);
        assert_eq!(s.stats().pixels_written, 512 * 512 + 256 * 256);
        assert_eq!(s.image().get_pixel(0, 0)[0], 99);
        assert_eq!(s.image().get_pixel(511, 511)[0], 40);
        // Hors de la zone commune avec `before` : inchangé.
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
        // Note 6G.1 : les chronos eux-mêmes avancent à chaque appel (même
        // vide) — la mesure a un coût, les compteurs d'événements non.
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
        // Tuile débordant à droite/bas : seuls les pixels dedans sont écrits
        // ET comptés (10×10, pas 256²).
        let mut s = PreviewSurface::new(100, 100);
        s.update_tile(&tile(90, 90, 256, 256, 7));
        assert_eq!(s.image().get_pixel(99, 99)[0], 7);
        assert_eq!(s.image().get_pixel(0, 0)[0], 0);
        assert_eq!(s.stats().tiles_applied, 1);
        assert_eq!(s.stats().pixels_written, 100);
    }
}

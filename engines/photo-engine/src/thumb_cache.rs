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

//! Key cache for layer thumbnails (Phase 6G.4).
//!
//! A stable key identifies a layer appearance state:
//! source + filters + masks + resize. A cache HIT avoids any
//! computation.

use std::sync::Arc;

/// Thumbnail cache key (versioned, comparable).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbnailKey {
    /// Source file hash (or `Arc::as_ptr` for empty images).
    pub source_ptr: u64,
    /// Applied-filter signature (u64 hash).
    pub filter_signature: u64,
    /// Mask count (0 if no mask).
    pub mask_count: usize,
    /// Target resize (width, height).
    pub dimensions: (u16, u16),
}

impl ThumbnailKey {
    /// Creates a key for the given layer.
    pub fn for_layer(layer: &crate::PixelLayer) -> Self {
        let source_ptr = Arc::as_ptr(&layer.source_image) as *const () as u64;
        let filter_signature = crate::filters_signature(&layer.filter_layers);
        let mask_count = layer.masks.len();
        Self {
            source_ptr,
            filter_signature,
            mask_count,
            dimensions: (48, 32),
        }
    }
}

/// Thumbnail cache statistics.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ThumbCacheStats {
    /// Hits (state already cached).
    pub hits: u64,
    /// Misses (computation needed).
    pub misses: u64,
}

impl ThumbCacheStats {
    /// Returns the total request count.
    pub fn total(&self) -> u64 {
        self.hits + self.misses
    }

    /// Returns the hit rate (0.0 if no requests).
    #[must_use]
    pub fn hit_rate(&self) -> f32 {
        if self.total() == 0 {
            0.0
        } else {
            self.hits as f32 / self.total() as f32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::PixelLayer;
    use std::sync::Arc;

    #[test]
    fn key_is_stable_for_same_layer() {
        let layer1 = PixelLayer::new("test", Arc::new(image::DynamicImage::new_rgba8(64, 64)));
        let key1 = ThumbnailKey::for_layer(&layer1);
        let key2 = ThumbnailKey::for_layer(&layer1);
        assert_eq!(key1, key2);
    }

    #[test]
    fn key_differs_for_different_sources() {
        let layer1 = PixelLayer::new("test", Arc::new(image::DynamicImage::new_rgba8(64, 64)));
        let layer2 = PixelLayer::new("test", Arc::new(image::DynamicImage::new_rgba8(32, 32)));
        let key1 = ThumbnailKey::for_layer(&layer1);
        let key2 = ThumbnailKey::for_layer(&layer2);
        assert_ne!(key1.source_ptr, key2.source_ptr);
    }
}

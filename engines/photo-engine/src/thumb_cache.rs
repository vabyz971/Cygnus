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

//! Cache clé pour miniatures de calques (Phase 6G.4).
//!
//! Une clé stable identifie un état d'apparence de calque :
//! source + filtres + masques + redimensionnement.
//! Un HIT du cache évite tout calcul.

use std::sync::Arc;

/// Clé de cache miniature (versionnée, comparable).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ThumbnailKey {
    /// Hash du fichier source (ou `Arc::as_ptr` pour images vides).
    pub source_ptr: u64,
    /// Signature des filtres appliqués (hash u64).
    pub filter_signature: u64,
    /// Nombre de masques (0 si pas de masque).
    pub mask_count: usize,
    /// Redimensionnement cible (largeur, hauteur).
    pub dimensions: (u16, u16),
}

impl ThumbnailKey {
    /// Crée une clé pour le calque donné.
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

/// Statistiques de cache miniature.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ThumbCacheStats {
    /// Hits (état déjà en cache).
    pub hits: u64,
    /// Misses (calcul nécessaire).
    pub misses: u64,
}

impl ThumbCacheStats {
    /// Retourne le nombre total de requêtes.
    pub fn total(&self) -> u64 {
        self.hits + self.misses
    }

    /// Retourne le taux de hits (0.0 si pas de requêtes).
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

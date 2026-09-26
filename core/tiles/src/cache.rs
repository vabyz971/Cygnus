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

//! Cache de tuiles : emplacements versionnés, handles opaques.
//!
//! [`TileCache<H>`] associe `TileId → (révision, handle)` : hit ssi la
//! révision stockée ÉGALE la demandée ([`TileKey`] conceptuel). Le type de
//! handle `H` sépare STRICTEMENT les mondes :
//!
//! - [`CpuTile`] (pixels partagés `Arc<[u8]>`) — [`CpuTileCache`] ;
//! - [`GpuTile`] (jeton opaque `token`, résolu en texture par le backend)
//!   — [`GpuTileCache`].
//!
//! Aucun pixel ne transite ici vers le GPU et aucun readback n'en revient :
//! le backend possède la table `jeton → texture` et libère ses textures
//! quand `insert` lui rend un handle évincé (valeur de retour — pas de
//! rappel, pas de synchronisation implicite).
//!
//! Éviction LRU bornée (compteur monotone + balayage du minimum) :
//! fondation volontairement simple — les caches backend affinés feront
//! mieux, avec la même interface `get/insert/invalidate`.

use std::collections::HashMap;
use std::sync::Arc;

use crate::{TileId, TileRevision};

/// Tuile CPU : pixels partagés (zéro copie vers les lecteurs).
#[derive(Debug, Clone)]
pub struct CpuTile {
    /// Pixels RGBA8 partagés (lignes majeures).
    pub data: Arc<[u8]>,
    /// Largeur en pixels.
    pub width: u32,
    /// Hauteur en pixels.
    pub height: u32,
}

/// Cache de tuiles CPU.
pub type CpuTileCache = TileCache<CpuTile>;

/// Poignée GPU opaque : le backend seul sait la résoudre en texture.
///
/// `token` est attribué par le backend à la création ; `generation`
/// change à chaque ré-upload (permet de détecter les jetons périmés sans
/// toucher au GPU).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct GpuTile {
    /// Jeton attribué par le backend.
    pub token: u64,
    /// Génération du contenu téléversé.
    pub generation: u64,
}

/// Cache de tuiles GPU (jetons — textures chez le backend).
pub type GpuTileCache = TileCache<GpuTile>;

/// Entrée versionnée (révision + handle + récence LRU).
#[derive(Debug, Clone)]
struct CacheEntry<H> {
    revision: TileRevision,
    handle: H,
    last_used: u64,
}

/// Statistiques du cache (observabilité, sans effet).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Entrées stockées.
    pub len: usize,
    /// Capacité configurée.
    pub capacity: usize,
    /// Lectures réussies (même révision).
    pub hits: u64,
    /// Lectures ratées (absent ou révision différente).
    pub misses: u64,
    /// Handles évincés (rendus à l'appelant via `insert`).
    pub evicted: u64,
}

/// Cache LRU borné de tuiles versionnées, paramétré par le handle.
///
/// `H` n'est jamais inspecté ici (stockage opaque) : CPU et GPU ne se
/// mélangent pas — ce sont deux instances de types distincts.
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
    /// Nouveau cache (`capacity` = entrées max ; 0 = ne stocke rien).
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

    /// Capacité configurée.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Entrées stockées.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Vrai si vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Statistiques cumulées.
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

    /// Lecture : hit ssi l'entrée existe ET sa révision égale `revision`.
    /// Marque la récence LRU (d'où `&mut`).
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

    /// Révision stockée pour l'emplacement (`None` si absent).
    #[must_use]
    pub fn revision_of(&self, id: TileId) -> Option<TileRevision> {
        self.entries.get(&id).map(|e| e.revision)
    }

    /// Stocke (ou remplace) un handle. Si le cache est plein et l'id est
    /// nouveau, le handle le MOINS récemment utilisé est rendu à
    /// l'appelant (le backend y libère sa texture — aucune fuite
    /// silencieuse). Capacité 0 : rend immédiatement `handle`.
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

    /// Retire un emplacement et rend son handle (`None` si absent — le
    /// backend n'a alors rien à libérer).
    pub fn invalidate(&mut self, id: TileId) -> Option<H> {
        self.entries.remove(&id).map(|e| e.handle)
    }

    /// Vide le cache et rend tous les handles (l'appelant les libère).
    pub fn invalidate_all(&mut self) -> Vec<H> {
        self.entries
            .drain()
            .map(|(_, entry)| entry.handle)
            .collect()
    }

    /// Évince le moins récemment utilisé (interne — le handle remonte).
    fn evict_lru(&mut self) -> Option<H> {
        let victim = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_used)
            .map(|(id, _)| *id)?;
        self.entries.remove(&victim).map(|e| e.handle)
    }
}

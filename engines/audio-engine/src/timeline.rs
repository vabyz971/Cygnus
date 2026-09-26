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

//! Timeline audio : régions de sources, décision temporelle.
//!
//! [`AudioTimeline::active_at`] répond « quelles sources jouent au temps
//! `t` ? » — purement éditorial, sans échantillons (le graphe DSP et son
//! backend produisent le son). Propre à l'audio : pas de `Scene`, pas de
//! pixels, pas de temps partagé avec la vidéo (deux domaines, deux
//! horloges — documenté, pas factorisé en douce).

use crate::AudioId;

/// Région : une source audible sur `[start, start + duration)`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Region {
    /// Nœud source joué.
    pub source: AudioId,
    /// Début (secondes).
    pub start: f32,
    /// Durée (secondes).
    pub duration: f32,
}

impl Region {
    /// La région couvre-t-elle `t` (début inclus, fin exclue) ?
    #[must_use]
    pub fn covers(&self, t: f32) -> bool {
        t.is_finite() && t >= self.start && t < self.start + self.duration
    }
}

/// Timeline : régions dans l'ordre d'ajout (index élevé = dessus de mix).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioTimeline {
    regions: Vec<Region>,
}

impl AudioTimeline {
    /// Timeline vide.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Vrai si vide.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// Nombre de régions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// Ajoute une région (durée ≤ 0 ou début infini : ignorée, faux).
    pub fn add_region(&mut self, source: AudioId, start: f32, duration: f32) -> bool {
        if !start.is_finite() || duration <= 0.0 {
            return false;
        }
        self.regions.push(Region {
            source,
            start,
            duration,
        });
        true
    }

    /// Sources audibles à `t`, dans l'ordre des régions.
    #[must_use]
    pub fn active_at(&self, t: f32) -> Vec<AudioId> {
        self.regions
            .iter()
            .filter(|r| r.covers(t))
            .map(|r| r.source)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regions_chevauchements_et_trous() {
        let mut timeline = AudioTimeline::new();
        assert!(timeline.is_empty());
        let a = AudioId::new();
        let b = AudioId::new();
        assert!(!timeline.add_region(a, 0.0, 0.0));
        assert!(!timeline.add_region(a, f32::NAN, 2.0));
        assert!(timeline.add_region(a, 0.0, 4.0));
        assert!(timeline.add_region(b, 2.0, 4.0));
        assert_eq!(timeline.active_at(1.0), vec![a]);
        assert_eq!(timeline.active_at(3.0), vec![a, b]);
        assert!(timeline.active_at(6.0).is_empty());
        assert_eq!(timeline.active_at(0.0), vec![a]);
    }
}

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

//! Nœuds métier audio : sources, gains, EQ, compresseur, reverb, mixeur.
//!
//! Données pures et validées (bornes à la construction) : le DSP vit dans
//! [`DspBackend`](crate::DspBackend), jamais ici. Aucun type visuel
//! (`Scene`, `Transform2D`, pixels) ne franchit cette frontière.

use ids::EntityId;

/// Identifiant stable d'un [`AudioNode`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct AudioId(EntityId);

impl AudioId {
    /// Nouvel identifiant aléatoire.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Identifiant nul (sentinelle, jamais attribué).
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// L'entité sous-jacente.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Enveloppe une entité existante.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for AudioId {
    /// Identifiant nul — même valeur que [`AudioId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for AudioId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "au{}", self.0.as_uuid())
    }
}

/// Forme d'onde d'un oscillateur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
    /// Sinusoïdale.
    Sine,
    /// Carrée.
    Square,
    /// Dent de scie.
    Saw,
    /// Triangulaire.
    Triangle,
}

/// Source sonore.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Échantillon nommé (chargé par le backend).
    Sample {
        /// Nom/chemin logique.
        name: String,
    },
    /// Oscillateur.
    Oscillator {
        /// Fréquence Hz (> 0).
        freq: f32,
        /// Forme.
        wave: Wave,
    },
}

/// Nœud métier : nature + paramètres validés.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioNodeKind {
    /// Source (échantillon ou oscillateur).
    Source(SourceKind),
    /// Gain en dB (−60..=+12).
    Gain {
        /// Décibels.
        db: f32,
    },
    /// Égaliseur 3 bandes (gains dB −24..=+24).
    Eq {
        /// Graves.
        low: f32,
        /// Médiums.
        mid: f32,
        /// Aigus.
        high: f32,
    },
    /// Compresseur (seuil dB + ratio).
    Compressor {
        /// Seuil en dB (−60..=0).
        threshold_db: f32,
        /// Ratio (≥ 1).
        ratio: f32,
    },
    /// Réverbération (mix 0..=1, déclin ≥ 0).
    Reverb {
        /// Dosage effet.
        mix: f32,
        /// Temps de déclin (s).
        decay: f32,
    },
    /// Mixeur (somme des entrées).
    Mixer,
}

/// Nœud audio : identité + nature + interrupteur.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AudioNode {
    /// Identifiant stable.
    pub id: AudioId,
    /// Nom d'affichage.
    pub name: String,
    /// Nature et paramètres.
    pub kind: AudioNodeKind,
    /// Faux = bypassé (réglages conservés).
    pub enabled: bool,
}

impl AudioNode {
    /// Gain validé (−60..=+12 dB).
    #[must_use]
    pub fn gain(name: impl Into<String>, db: f32) -> Self {
        Self::named(
            name,
            AudioNodeKind::Gain {
                db: db.clamp(-60.0, 12.0),
            },
        )
    }

    /// Égaliseur validé (±24 dB par bande).
    #[must_use]
    pub fn eq(name: impl Into<String>, low: f32, mid: f32, high: f32) -> Self {
        let clamp = |v: f32| v.clamp(-24.0, 24.0);
        Self::named(
            name,
            AudioNodeKind::Eq {
                low: clamp(low),
                mid: clamp(mid),
                high: clamp(high),
            },
        )
    }

    /// Compresseur validé (seuil −60..=0 dB, ratio ≥ 1).
    #[must_use]
    pub fn compressor(name: impl Into<String>, threshold_db: f32, ratio: f32) -> Self {
        Self::named(
            name,
            AudioNodeKind::Compressor {
                threshold_db: threshold_db.clamp(-60.0, 0.0),
                ratio: ratio.max(1.0),
            },
        )
    }

    /// Réverbération validée (mix 0..=1, déclin ≥ 0).
    #[must_use]
    pub fn reverb(name: impl Into<String>, mix: f32, decay: f32) -> Self {
        Self::named(
            name,
            AudioNodeKind::Reverb {
                mix: mix.clamp(0.0, 1.0),
                decay: decay.max(0.0),
            },
        )
    }

    /// Mixeur.
    #[must_use]
    pub fn mixer(name: impl Into<String>) -> Self {
        Self::named(name, AudioNodeKind::Mixer)
    }

    /// Source échantillon.
    #[must_use]
    pub fn sample(name: impl Into<String>, sample: impl Into<String>) -> Self {
        Self::named(
            name,
            AudioNodeKind::Source(SourceKind::Sample {
                name: sample.into(),
            }),
        )
    }

    /// Oscillateur (fréquence > 0).
    #[must_use]
    pub fn oscillator(name: impl Into<String>, freq: f32, wave: Wave) -> Self {
        Self::named(
            name,
            AudioNodeKind::Source(SourceKind::Oscillator {
                freq: freq.max(0.0),
                wave,
            }),
        )
    }

    fn named(name: impl Into<String>, kind: AudioNodeKind) -> Self {
        Self {
            id: AudioId::new(),
            name: name.into(),
            kind,
            enabled: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parametres_bornes_a_la_construction() {
        assert_eq!(
            AudioNode::gain("g", 99.0).kind,
            AudioNodeKind::Gain { db: 12.0 }
        );
        assert_eq!(
            AudioNode::eq("e", -99.0, 0.0, 99.0).kind,
            AudioNodeKind::Eq {
                low: -24.0,
                mid: 0.0,
                high: 24.0
            }
        );
        assert_eq!(
            AudioNode::compressor("c", 5.0, 0.5).kind,
            AudioNodeKind::Compressor {
                threshold_db: 0.0,
                ratio: 1.0
            }
        );
        assert_eq!(
            AudioNode::reverb("r", 2.0, -1.0).kind,
            AudioNodeKind::Reverb {
                mix: 1.0,
                decay: 0.0
            }
        );
        assert_ne!(AudioNode::mixer("m").id, AudioId::nil());
    }
}

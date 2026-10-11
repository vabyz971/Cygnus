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

//! Audio business nodes: sources, gains, EQ, compressor, reverb, mixer.
//!
//! Pure validated data (bounds at construction): DSP lives in
//! [`DspBackend`](crate::DspBackend), never here. No visual type
//! (`Scene`, `Transform2D`, pixels) crosses this boundary.

use ids::EntityId;

/// Stable identifier of an [`AudioNode`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct AudioId(EntityId);

impl AudioId {
    /// New random identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Null identifier (sentinel, never assigned).
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// The underlying entity.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Wraps an existing entity.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for AudioId {
    /// Null identifier — same value as [`AudioId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for AudioId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "au{}", self.0.as_uuid())
    }
}

/// Oscillator waveform.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Wave {
    /// Sine.
    Sine,
    /// Square.
    Square,
    /// Sawtooth.
    Saw,
    /// Triangle.
    Triangle,
}

/// Sound source.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    /// Named sample (loaded by the backend).
    Sample {
        /// Logical name/path.
        name: String,
    },
    /// Oscillator.
    Oscillator {
        /// Frequency in Hz (> 0).
        freq: f32,
        /// Waveform.
        wave: Wave,
    },
}

/// Business node: kind + validated parameters.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AudioNodeKind {
    /// Source (sample or oscillator).
    Source(SourceKind),
    /// Gain in dB (−60..=+12).
    Gain {
        /// Decibels.
        db: f32,
    },
    /// 3-band equalizer (dB gains −24..=+24).
    Eq {
        /// Lows.
        low: f32,
        /// Mids.
        mid: f32,
        /// Highs.
        high: f32,
    },
    /// Compressor (dB threshold + ratio).
    Compressor {
        /// Threshold in dB (−60..=0).
        threshold_db: f32,
        /// Ratio (≥ 1).
        ratio: f32,
    },
    /// Reverb (mix 0..=1, decay ≥ 0).
    Reverb {
        /// Effect amount.
        mix: f32,
        /// Decay time (s).
        decay: f32,
    },
    /// Mixer (sum of inputs).
    Mixer,
}

/// Audio node: identity + kind + switch.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct AudioNode {
    /// Stable identifier.
    pub id: AudioId,
    /// Display name.
    pub name: String,
    /// Kind and parameters.
    pub kind: AudioNodeKind,
    /// False = bypassed (settings kept).
    pub enabled: bool,
}

impl AudioNode {
    /// Validated gain (−60..=+12 dB).
    #[must_use]
    pub fn gain(name: impl Into<String>, db: f32) -> Self {
        Self::named(
            name,
            AudioNodeKind::Gain {
                db: db.clamp(-60.0, 12.0),
            },
        )
    }

    /// Validated equalizer (±24 dB per band).
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

    /// Validated compressor (−60..=0 dB threshold, ratio ≥ 1).
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

    /// Validated reverb (mix 0..=1, decay ≥ 0).
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

    /// Mixer.
    #[must_use]
    pub fn mixer(name: impl Into<String>) -> Self {
        Self::named(name, AudioNodeKind::Mixer)
    }

    /// Sample source.
    #[must_use]
    pub fn sample(name: impl Into<String>, sample: impl Into<String>) -> Self {
        Self::named(
            name,
            AudioNodeKind::Source(SourceKind::Sample {
                name: sample.into(),
            }),
        )
    }

    /// Oscillator (frequency > 0).
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

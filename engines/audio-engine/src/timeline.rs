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

//! Audio timeline: source regions, time decisions.
//!
//! [`AudioTimeline::active_at`] answers "which sources play at time
//! `t`?" — purely editorial, no samples (the DSP graph and its
//! backend produce the sound). Audio-specific: no `Scene`, no
//! pixels, no time shared with video (two domains, two
//! clocks — documented, not silently factored).

use crate::AudioId;

/// Region: an audible source over `[start, start + duration)`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Region {
    /// Source node played.
    pub source: AudioId,
    /// Start (seconds).
    pub start: f32,
    /// Duration (seconds).
    pub duration: f32,
}

impl Region {
    /// Does the region cover `t` (start inclusive, end exclusive)?
    #[must_use]
    pub fn covers(&self, t: f32) -> bool {
        t.is_finite() && t >= self.start && t < self.start + self.duration
    }
}

/// Timeline: regions in insertion order (high index = mix top).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AudioTimeline {
    regions: Vec<Region>,
}

impl AudioTimeline {
    /// Empty timeline.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.regions.is_empty()
    }

    /// Number of regions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.regions.len()
    }

    /// Adds a region (duration ≤ 0 or infinite start: ignored, false).
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

    /// Audible sources at `t`, in region order.
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

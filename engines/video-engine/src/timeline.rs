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

//! Editorial timeline: tracks, regions, frame decisions.
//!
//! [`Timeline::active_at`] answers the only editorial question that matters
//! here: "which clips cover time `t`, and at which clip time?"
//! (speed included). Neither decoding nor compositing — the backend will request
//! frames from the decoders then fold them per [`CompositeDesc`](crate::CompositeDesc).

use crate::ClipId;

/// Clip region on a track: placement + playback speed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TimelineClip {
    /// Source clip.
    pub clip: ClipId,
    /// Start on the timeline (seconds).
    pub start: f32,
    /// Taken duration (timeline seconds).
    pub duration: f32,
    /// Speed (1 = normal, 0.5 = slow motion…; ≤ 0 = frozen on the first frame).
    pub speed: f32,
}

impl TimelineClip {
    /// Clip time for a timeline time (`None` if outside the region).
    #[must_use]
    pub fn clip_pts_at(&self, t: f32) -> Option<f32> {
        if !t.is_finite() || t < self.start || t >= self.start + self.duration {
            return None;
        }
        if self.speed <= 0.0 {
            return Some(0.0);
        }
        Some((t - self.start) * self.speed)
    }
}

/// Track: ordered regions (index 0 = bottom of stack).
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Track {
    /// Display name.
    pub name: String,
    /// Regions.
    pub clips: Vec<TimelineClip>,
}

/// Active clip at a time: track + clip time.
#[derive(Debug, Clone, PartialEq)]
pub struct ActiveClip {
    /// Track index (0 = bottom).
    pub track: usize,
    /// Source clip.
    pub clip: ClipId,
    /// Time within the clip (seconds).
    pub clip_pts: f32,
}

/// Timeline: stacked tracks (high index = top).
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct Timeline {
    tracks: Vec<Track>,
}

impl Timeline {
    /// Empty timeline.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of tracks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// True if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tracks.is_empty()
    }

    /// Adds a track (top of stack) and returns its index.
    pub fn add_track(&mut self, name: impl Into<String>) -> usize {
        self.tracks.push(Track {
            name: name.into(),
            clips: Vec::new(),
        });
        self.tracks.len() - 1
    }

    /// Places a region on a track (false if unknown track).
    pub fn place(&mut self, track: usize, clip: TimelineClip) -> bool {
        match self.tracks.get_mut(track) {
            Some(slot) => {
                slot.clips.push(clip);
                true
            }
            None => false,
        }
    }

    /// Clips covering `t`, tracks in order (bottom → top),
    /// regions in placement order.
    #[must_use]
    pub fn active_at(&self, t: f32) -> Vec<ActiveClip> {
        let mut active = Vec::new();
        for (track, slot) in self.tracks.iter().enumerate() {
            for region in &slot.clips {
                if let Some(clip_pts) = region.clip_pts_at(t) {
                    active.push(ActiveClip {
                        track,
                        clip: region.clip,
                        clip_pts,
                    });
                }
            }
        }
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip_at(start: f32, duration: f32, speed: f32) -> TimelineClip {
        TimelineClip {
            clip: ClipId::new(),
            start,
            duration,
            speed,
        }
    }

    #[test]
    fn actifs_chevauchements_et_trous() {
        let mut timeline = Timeline::new();
        assert!(timeline.is_empty());
        let v1 = timeline.add_track("V1");
        let v2 = timeline.add_track("V2");
        assert!(!timeline.place(9, clip_at(0.0, 5.0, 1.0)));
        timeline.place(v1, clip_at(0.0, 5.0, 1.0));
        timeline.place(v2, clip_at(3.0, 5.0, 1.0));

        assert_eq!(timeline.active_at(1.0).len(), 1);
        let both = timeline.active_at(4.0);
        assert_eq!(both.len(), 2);
        assert_eq!((both[0].track, both[1].track), (0, 1));
        assert!(timeline.active_at(20.0).is_empty());
        // Bounds: start inclusive, end exclusive.
        assert_eq!(timeline.active_at(0.0).len(), 1);
        assert_eq!(timeline.active_at(8.0).len(), 0);
    }

    #[test]
    fn vitesse_et_fige() {
        let mut timeline = Timeline::new();
        let v = timeline.add_track("V");
        let id = ClipId::new();
        timeline.place(
            v,
            TimelineClip {
                clip: id,
                start: 10.0,
                duration: 4.0,
                speed: 0.5,
            },
        );
        let active = timeline.active_at(12.0);
        assert_eq!(active.len(), 1);
        assert!((active[0].clip_pts - 1.0).abs() < 0.001);

        let mut frozen = Timeline::new();
        let f = frozen.add_track("F");
        frozen.place(
            f,
            TimelineClip {
                clip: id,
                start: 0.0,
                duration: 2.0,
                speed: 0.0,
            },
        );
        assert_eq!(frozen.active_at(1.0)[0].clip_pts, 0.0);
    }
}

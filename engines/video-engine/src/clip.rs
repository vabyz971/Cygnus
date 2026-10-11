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

//! Clips and frames: time descriptors, no pixels.
//!
//! A [`Clip`] describes a media (duration, frame rate, dimensions); a
//! [`FrameDescriptor`] designates a frame to decode (clip + index + time).
//! Pixels stay with the decoders (see [`FrameDecoder`](crate::FrameDecoder)).

use ids::EntityId;

/// Stable identifier of a [`Clip`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct ClipId(EntityId);

impl ClipId {
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

impl Default for ClipId {
    /// Null identifier — same value as [`ClipId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for ClipId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vc{}", self.0.as_uuid())
    }
}

/// Source clip: media + frame rate + dimensions (no pixels here).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Clip {
    /// Stable identifier.
    pub id: ClipId,
    /// Display name.
    pub name: String,
    /// Duration in seconds (> 0 expected).
    pub duration_secs: f32,
    /// Frames per second (> 0 expected).
    pub fps: f32,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl Clip {
    /// New clip (non-finite or zero duration/rate → 0, size kept).
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        duration_secs: f32,
        fps: f32,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            id: ClipId::new(),
            name: name.into(),
            duration_secs: sanitize_pos(duration_secs),
            fps: sanitize_pos(fps),
            width,
            height,
        }
    }

    /// Number of frames (0 if zero duration or rate).
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        if self.duration_secs <= 0.0 || self.fps <= 0.0 {
            return 0;
        }
        (f64::from(self.duration_secs) * f64::from(self.fps)).floor() as u64
    }

    /// Is time (seconds, relative to the clip) covered?
    /// (`0 <= pts < duration` — the end bound belongs to the next
    /// frame / to none).
    #[must_use]
    pub fn contains_pts(&self, pts: f32) -> bool {
        pts.is_finite() && pts >= 0.0 && pts < self.duration_secs
    }

    /// Frame index for a time (`None` if outside the clip).
    #[must_use]
    pub fn frame_at(&self, pts: f32) -> Option<u64> {
        if !self.contains_pts(pts) {
            return None;
        }
        Some((f64::from(pts) * f64::from(self.fps)).floor() as u64)
    }
}

fn sanitize_pos(v: f32) -> f32 {
    if v.is_finite() { v.max(0.0) } else { 0.0 }
}

/// Frame to decode: time address + expected dimensions.
/// Pixels are produced by the decoder, never stored here.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FrameDescriptor {
    /// Source clip.
    pub clip: ClipId,
    /// Frame index.
    pub index: u64,
    /// Time in seconds (relative to the clip).
    pub pts: f32,
    /// Expected width.
    pub width: u32,
    /// Expected height.
    pub height: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_images_et_bornes() {
        let clip = Clip::new("rushes", 10.0, 25.0, 1920, 1080);
        assert_eq!(clip.frame_count(), 250);
        assert!(clip.contains_pts(0.0));
        assert!(clip.contains_pts(9.999));
        assert!(!clip.contains_pts(10.0));
        assert!(!clip.contains_pts(-0.1));
        assert_eq!(clip.frame_at(0.0), Some(0));
        assert_eq!(clip.frame_at(9.999), Some(249));
        assert_eq!(clip.frame_at(10.0), None);
        assert_eq!(clip.frame_at(f32::NAN), None);
    }

    #[test]
    fn clip_degenere_sans_images() {
        let clip = Clip::new("vide", 0.0, 25.0, 640, 480);
        assert_eq!(clip.frame_count(), 0);
        assert!(!clip.contains_pts(0.0));
    }
}

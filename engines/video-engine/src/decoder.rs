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

//! Decoding: the trait real readers will implement.
//!
//! The engine decodes NOTHING itself (no full reader here): it
//! addresses frames ([`FrameDescriptor`]) and lets the backend
//! produce them. [`NullDecoder`] answers synthetically (tests, mockups).

use crate::{Clip, FrameDescriptor};

/// Frame decoder: time address → descriptor ready to produce.
/// The real implementation lives outside this crate (ffmpeg/GStreamer…).
pub trait FrameDecoder {
    /// Descriptor of the frame at time `pts` (relative to the clip),
    /// `None` if outside the clip. Decodes no pixels: the backend requests
    /// pixels afterwards, per descriptor.
    fn frame_at(&self, clip: &Clip, pts: f32) -> Option<FrameDescriptor>;
}

/// Null decoder: synthetic descriptors aligned to the clip
/// (dimensions copied, index computed — no pixels).
#[derive(Debug, Clone, Copy, Default)]
pub struct NullDecoder;

impl FrameDecoder for NullDecoder {
    fn frame_at(&self, clip: &Clip, pts: f32) -> Option<FrameDescriptor> {
        let index = clip.frame_at(pts)?;
        Some(FrameDescriptor {
            clip: clip.id,
            index,
            pts,
            width: clip.width,
            height: clip.height,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodeur_nul_cale_et_borne() {
        let clip = Clip::new("c", 4.0, 25.0, 1280, 720);
        let frame = NullDecoder.frame_at(&clip, 1.0).expect("dedans");
        assert_eq!((frame.index, frame.pts), (25, 1.0));
        assert_eq!((frame.width, frame.height), (1280, 720));
        assert_eq!(frame.clip, clip.id);
        assert!(NullDecoder.frame_at(&clip, 4.0).is_none());
        assert!(NullDecoder.frame_at(&clip, -1.0).is_none());
    }
}

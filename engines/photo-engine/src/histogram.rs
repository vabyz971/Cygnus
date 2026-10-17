// Cygnus — Professional open-source creative suite
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

//! RGB + luminance histogram (O006) over an RGBA8 buffer (the capped
//! composite preview: cheap, never the full-resolution document).
//!
//! Large buffers are sampled on a regular stride so a refresh stays
//! well under a millisecond-scale budget; fully transparent pixels
//! (infinite canvas around the document) are skipped.

/// Per-channel bin counts (256 bins each).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Histogram {
    pub red: [u32; 256],
    pub green: [u32; 256],
    pub blue: [u32; 256],
    /// Rec. 601 luma.
    pub luma: [u32; 256],
    /// Pixels counted.
    pub samples: u32,
}

impl Default for Histogram {
    fn default() -> Self {
        Self {
            red: [0; 256],
            green: [0; 256],
            blue: [0; 256],
            luma: [0; 256],
            samples: 0,
        }
    }
}

/// Sampling budget: beyond it pixels are skipped on a regular stride.
pub const MAX_SAMPLES: usize = 262_144;

impl Histogram {
    /// Builds the histogram of an RGBA8 buffer (`rgba.len()` multiple
    /// of 4; a trailing partial pixel is ignored).
    #[must_use]
    pub fn from_rgba(rgba: &[u8]) -> Self {
        let mut h = Self::default();
        let pixels = rgba.len() / 4;
        let stride = pixels.div_ceil(MAX_SAMPLES).max(1);
        for px in rgba.as_chunks::<4>().0.iter().step_by(stride) {
            if px[3] == 0 {
                continue;
            }
            h.red[usize::from(px[0])] += 1;
            h.green[usize::from(px[1])] += 1;
            h.blue[usize::from(px[2])] += 1;
            let y =
                (299 * u32::from(px[0]) + 587 * u32::from(px[1]) + 114 * u32::from(px[2]) + 500)
                    / 1000;
            h.luma[y.min(255) as usize] += 1;
            h.samples += 1;
        }
        h
    }

    /// Highest bin across all channels (for normalized drawing; 0 when
    /// empty).
    #[must_use]
    pub fn peak(&self) -> u32 {
        self.red
            .iter()
            .chain(self.green.iter())
            .chain(self.blue.iter())
            .chain(self.luma.iter())
            .copied()
            .max()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_each_channel() {
        let rgba = [255, 0, 10, 255, 255, 0, 10, 255, 0, 128, 10, 255];
        let h = Histogram::from_rgba(&rgba);
        assert_eq!(h.samples, 3);
        assert_eq!(h.red[255], 2);
        assert_eq!(h.red[0], 1);
        assert_eq!(h.green[128], 1);
        assert_eq!(h.blue[10], 3);
        assert_eq!(h.luma.iter().sum::<u32>(), 3);
        assert_eq!(h.peak(), 3);
    }

    #[test]
    fn transparent_pixels_are_skipped() {
        let rgba = [255, 255, 255, 0, 10, 10, 10, 255];
        let h = Histogram::from_rgba(&rgba);
        assert_eq!(h.samples, 1);
        assert_eq!(h.luma[10], 1);
    }

    #[test]
    fn gray_luma_matches_value() {
        let h = Histogram::from_rgba(&[77, 77, 77, 255]);
        assert_eq!(h.luma[77], 1);
    }

    #[test]
    fn large_buffers_are_sampled() {
        let rgba = vec![50u8; (MAX_SAMPLES * 4 + 7) * 4];
        let h = Histogram::from_rgba(&rgba);
        assert!(h.samples as usize <= MAX_SAMPLES);
        assert!(h.samples > 0);
    }

    #[test]
    fn empty_buffer() {
        let h = Histogram::from_rgba(&[]);
        assert_eq!(h.samples, 0);
        assert_eq!(h.peak(), 0);
    }
}

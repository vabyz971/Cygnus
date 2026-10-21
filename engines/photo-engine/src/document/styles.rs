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

//! Layer styles (first two): drop shadow and outer glow.
//!
//! Non-destructive and derived from the layer's final appearance: the
//! compositor turns the layer's DOCUMENT-space raster (after filters,
//! masks and transform) into underlays — alpha blurred, tinted, shifted
//! — blended just below the layer itself ([`underlays`]). Sizes are in
//! document pixels; [`LayerStyles::scaled`] follows display-scale
//! compositing. The GPU path does not draw styles yet: styled
//! documents fall back to the CPU composite.

use image::{ImageBuffer, Rgba};
use serde::{Deserialize, Serialize};

use super::model::BlendMode;

/// Largest style size or distance (document pixels).
pub const MAX_STYLE_SIZE: f32 = 250.0;

/// Shadow cast below the layer, away from the light.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DropShadow {
    /// Drawn or not (settings kept when off).
    pub enabled: bool,
    /// Shadow color.
    pub color: [u8; 3],
    /// Opacity 0..=1.
    pub opacity: f32,
    /// Light direction in degrees (120 = from the upper left: the
    /// shadow falls toward the lower right).
    pub angle: f32,
    /// Offset length, document pixels.
    pub distance: f32,
    /// Blur size, document pixels.
    pub size: f32,
}

impl Default for DropShadow {
    fn default() -> Self {
        Self {
            enabled: false,
            color: [0, 0, 0],
            opacity: 0.6,
            angle: 120.0,
            distance: 12.0,
            size: 16.0,
        }
    }
}

/// Soft halo around the layer.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct OuterGlow {
    /// Drawn or not (settings kept when off).
    pub enabled: bool,
    /// Glow color.
    pub color: [u8; 3],
    /// Opacity 0..=1.
    pub opacity: f32,
    /// Blur size, document pixels.
    pub size: f32,
}

impl Default for OuterGlow {
    fn default() -> Self {
        Self {
            enabled: false,
            color: [255, 236, 170],
            opacity: 0.75,
            size: 24.0,
        }
    }
}

/// Styles of one layer.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct LayerStyles {
    /// Drop shadow.
    pub drop_shadow: DropShadow,
    /// Outer glow.
    pub outer_glow: OuterGlow,
}

impl LayerStyles {
    /// True when at least one style draws something.
    #[must_use]
    pub fn is_active(&self) -> bool {
        (self.drop_shadow.enabled && self.drop_shadow.opacity > 0.0)
            || (self.outer_glow.enabled && self.outer_glow.opacity > 0.0)
    }

    /// Values brought into range (non-finite → defaults).
    #[must_use]
    pub fn sanitized(self) -> Self {
        let pick = |v: f32, lo: f32, hi: f32, fallback: f32| {
            if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            }
        };
        let (shadow, glow) = (DropShadow::default(), OuterGlow::default());
        Self {
            drop_shadow: DropShadow {
                opacity: pick(self.drop_shadow.opacity, 0.0, 1.0, shadow.opacity),
                angle: pick(self.drop_shadow.angle, -360.0, 360.0, shadow.angle),
                distance: pick(
                    self.drop_shadow.distance,
                    0.0,
                    MAX_STYLE_SIZE,
                    shadow.distance,
                ),
                size: pick(self.drop_shadow.size, 0.0, MAX_STYLE_SIZE, shadow.size),
                ..self.drop_shadow
            },
            outer_glow: OuterGlow {
                opacity: pick(self.outer_glow.opacity, 0.0, 1.0, glow.opacity),
                size: pick(self.outer_glow.size, 0.0, MAX_STYLE_SIZE, glow.size),
                ..self.outer_glow
            },
        }
    }

    /// Same styles at display scale `s` (sizes and distances follow).
    #[must_use]
    pub fn scaled(self, s: f32) -> Self {
        Self {
            drop_shadow: DropShadow {
                distance: self.drop_shadow.distance * s,
                size: self.drop_shadow.size * s,
                ..self.drop_shadow
            },
            outer_glow: OuterGlow {
                size: self.outer_glow.size * s,
                ..self.outer_glow
            },
        }
    }
}

/// One style layer to draw below the layer: `image` at the layer's
/// raster position shifted by `(dx, dy)`.
pub struct Underlay {
    /// Tinted, blurred coverage.
    pub image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    /// Horizontal shift from the layer raster origin (document pixels).
    pub dx: f32,
    /// Vertical shift from the layer raster origin (document pixels).
    pub dy: f32,
    /// Blend mode of the underlay.
    pub mode: BlendMode,
    /// Opacity 0..=1 (multiplied by the layer opacity by the caller).
    pub opacity: f32,
}

/// Underlays of `top` (the layer's document-space raster), bottom
/// first: shadow, then glow. Empty when no style is active.
#[must_use]
pub fn underlays(top: &ImageBuffer<Rgba<u8>, Vec<u8>>, styles: &LayerStyles) -> Vec<Underlay> {
    let styles = styles.sanitized();
    let mut out = Vec::new();
    let shadow = styles.drop_shadow;
    if shadow.enabled && shadow.opacity > 0.0 {
        let rad = shadow.angle.to_radians();
        // Light at `angle`: the shadow goes the other way (y down).
        let (sx, sy) = (-rad.cos() * shadow.distance, rad.sin() * shadow.distance);
        let (image, pad) = halo(top, shadow.size, shadow.color);
        out.push(Underlay {
            image,
            dx: sx - pad,
            dy: sy - pad,
            mode: BlendMode::Multiply,
            opacity: shadow.opacity,
        });
    }
    let glow = styles.outer_glow;
    if glow.enabled && glow.opacity > 0.0 {
        let (image, pad) = halo(top, glow.size, glow.color);
        out.push(Underlay {
            image,
            dx: -pad,
            dy: -pad,
            mode: BlendMode::Screen,
            opacity: glow.opacity,
        });
    }
    out
}

/// Alpha of `top` padded by the blur reach, blurred to `size` and
/// tinted `color`; returns the image and its padding.
fn halo(
    top: &ImageBuffer<Rgba<u8>, Vec<u8>>,
    size: f32,
    color: [u8; 3],
) -> (ImageBuffer<Rgba<u8>, Vec<u8>>, f32) {
    let radius = (size / 2.0).round().max(0.0) as usize;
    let pad = (radius * 3 + 1) as u32;
    let (w, h) = (top.width() + 2 * pad, top.height() + 2 * pad);
    let mut alpha = vec![0f32; w as usize * h as usize];
    for (x, y, p) in top.enumerate_pixels() {
        alpha[((y + pad) * w + x + pad) as usize] = f32::from(p[3]);
    }
    if radius > 0 {
        box_blur3(&mut alpha, w as usize, h as usize, radius);
    }
    let image = ImageBuffer::from_fn(w, h, |x, y| {
        let a = alpha[(y * w + x) as usize].round().clamp(0.0, 255.0) as u8;
        Rgba([color[0], color[1], color[2], a])
    });
    (image, pad as f32)
}

/// Three box-blur passes (≈ gaussian) over a `w` x `h` plane, zero
/// beyond the edges (the halo fades out into the padding).
fn box_blur3(values: &mut [f32], w: usize, h: usize, radius: usize) {
    let mut scratch = vec![0f32; values.len()];
    let norm = (2 * radius + 1) as f32;
    let pass = |src: &[f32], dst: &mut [f32], horizontal: bool| {
        let (lines, len) = if horizontal { (h, w) } else { (w, h) };
        let at = |line: usize, i: usize| {
            if horizontal {
                line * w + i
            } else {
                i * w + line
            }
        };
        for line in 0..lines {
            let sample = |i: isize| {
                if i < 0 || i >= len as isize {
                    0.0
                } else {
                    src[at(line, i as usize)]
                }
            };
            let mut sum: f32 = (-(radius as isize)..=radius as isize).map(sample).sum();
            for i in 0..len {
                dst[at(line, i)] = sum / norm;
                sum +=
                    sample(i as isize + radius as isize + 1) - sample(i as isize - radius as isize);
            }
        }
    };
    for _ in 0..3 {
        pass(values, &mut scratch, true);
        pass(&scratch, values, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 10 x 10 opaque white square.
    fn square() -> ImageBuffer<Rgba<u8>, Vec<u8>> {
        ImageBuffer::from_pixel(10, 10, Rgba([255, 255, 255, 255]))
    }

    #[test]
    fn inactive_styles_draw_nothing() {
        assert!(!LayerStyles::default().is_active());
        assert!(underlays(&square(), &LayerStyles::default()).is_empty());
    }

    #[test]
    fn drop_shadow_falls_away_from_the_light() {
        let styles = LayerStyles {
            drop_shadow: DropShadow {
                enabled: true,
                angle: 90.0,
                distance: 5.0,
                size: 0.0,
                ..DropShadow::default()
            },
            ..LayerStyles::default()
        };
        let layers = underlays(&square(), &styles);
        assert_eq!(layers.len(), 1);
        let shadow = &layers[0];
        // Light from the top (90°): shadow straight down, no blur.
        assert!((shadow.dx + 1.0).abs() < 1e-3, "pad only horizontally");
        assert!((shadow.dy - (5.0 - 1.0)).abs() < 1e-3);
        assert_eq!(shadow.mode, BlendMode::Multiply);
        assert_eq!(
            shadow.image.get_pixel(5, 5)[3],
            255,
            "solid under the square"
        );
        assert_eq!(shadow.image.get_pixel(0, 0)[3], 0, "padding stays clear");
    }

    #[test]
    fn glow_spreads_beyond_the_layer() {
        let styles = LayerStyles {
            outer_glow: OuterGlow {
                enabled: true,
                size: 8.0,
                ..OuterGlow::default()
            },
            ..LayerStyles::default()
        };
        let layers = underlays(&square(), &styles);
        assert_eq!(layers.len(), 1);
        let glow = &layers[0];
        let pad = (-glow.dx) as u32;
        assert!(pad > 4, "room for the halo");
        let outside = glow.image.get_pixel(pad - 2, pad + 5)[3];
        assert!(outside > 0 && outside < 255, "soft halo outside: {outside}");
        assert_eq!(glow.mode, BlendMode::Screen);
    }

    #[test]
    fn scaled_styles_follow_the_display_scale() {
        let styles = LayerStyles {
            drop_shadow: DropShadow {
                distance: 10.0,
                size: 20.0,
                ..DropShadow::default()
            },
            ..LayerStyles::default()
        }
        .scaled(0.5);
        assert_eq!(styles.drop_shadow.distance, 5.0);
        assert_eq!(styles.drop_shadow.size, 10.0);
    }
}

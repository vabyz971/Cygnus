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

//! Canonical 2D affine transform (scale → skew → rotation convention
//! around the center, then offset).
//!
//! SINGLE source of the formula: `photo-engine` (compositing) and `cygnus-ui`
//! (`image_canvas::corners`) call it — no local copy anywhere else.

use serde::{Deserialize, Serialize};

/// Affine transform applied AT DRAW time ("state-only" model: changing
/// a value never regenerates pixels). Operation order convention
/// (around the image center):
/// scale → skew → rotation → offset. `skew_x` shears X along Y (right
/// edge in X), `skew_y` shears Y along X, in degrees.
///
/// Backward-compatible serialization: legacy projects (uniform
/// `scale` field) are read via custom [`Deserialize`] — `scale` becomes
/// `scale_x == scale_y`. New fields take their defaults in JSONs
/// missing them (derived Deserialize accepts missing fields).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Transform2D {
    pub offset_x: f32,
    pub offset_y: f32,
    /// Rotation in degrees (clockwise), applied around the center
    pub rotation_deg: f32,
    /// Image X scale (1.0 = 100 %)
    pub scale_x: f32,
    /// Image Y scale (1.0 = 100 %)
    pub scale_y: f32,
    /// Horizontal skew in degrees (shearing X along Y)
    pub skew_x: f32,
    /// Vertical skew in degrees (shearing Y along X)
    pub skew_y: f32,
}

impl Default for Transform2D {
    /// Identity transform.
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            rotation_deg: 0.0,
            scale_x: 1.0,
            scale_y: 1.0,
            skew_x: 0.0,
            skew_y: 0.0,
        }
    }
}

impl<'de> Deserialize<'de> for Transform2D {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            OffsetX,
            OffsetY,
            RotationDeg,
            #[serde(rename = "scale")]
            ScaleLegacy,
            ScaleX,
            ScaleY,
            SkewX,
            SkewY,
        }

        struct Transform2DVisitor;

        impl<'de> serde::de::Visitor<'de> for Transform2DVisitor {
            type Value = Transform2D;

            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("struct Transform2D")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut t = Transform2D::default();
                let mut has_sx = false;
                let mut has_sy = false;
                let mut legacy_scale: Option<f32> = None;
                while let Some(key) = map.next_key::<Field>()? {
                    match key {
                        Field::OffsetX => t.offset_x = map.next_value()?,
                        Field::OffsetY => t.offset_y = map.next_value()?,
                        Field::RotationDeg => t.rotation_deg = map.next_value()?,
                        Field::ScaleX => {
                            t.scale_x = map.next_value()?;
                            has_sx = true;
                        }
                        Field::ScaleY => {
                            t.scale_y = map.next_value()?;
                            has_sy = true;
                        }
                        Field::SkewX => t.skew_x = map.next_value()?,
                        Field::SkewY => t.skew_y = map.next_value()?,
                        Field::ScaleLegacy => legacy_scale = Some(map.next_value::<f32>()?),
                    }
                }
                // Legacy project (uniform `scale`): propagate to the axes unless
                // a per-axis scale was already explicitly read.
                if let Some(s) = legacy_scale {
                    if !has_sx {
                        t.scale_x = s;
                    }
                    if !has_sy {
                        t.scale_y = s;
                    }
                }
                Ok(t)
            }
        }

        deserializer.deserialize_map(Transform2DVisitor)
    }
}

impl Transform2D {
    /// True when every field is finite (guard rail for worker
    /// commands carrying externally built transforms).
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.offset_x.is_finite()
            && self.offset_y.is_finite()
            && self.rotation_deg.is_finite()
            && self.scale_x.is_finite()
            && self.scale_y.is_finite()
            && self.skew_x.is_finite()
            && self.skew_y.is_finite()
    }

    /// Local image point (px, origin at top-left corner) → doc
    /// (canvas coordinates, doc center = (doc_w/2, doc_h/2)). Same
    /// convention as the canvas draw: the offset is the
    /// top-left corner of the scaled rectangle (before rotation) and
    /// shear/rotation happen around the scaled rectangle's center.
    #[must_use]
    pub fn local_to_doc(&self, w0: f32, h0: f32, x: f32, y: f32) -> (f32, f32) {
        let sx = self.scale_x;
        let sy = self.scale_y;
        let kx = self.skew_x.to_radians().tan();
        let ky = self.skew_y.to_radians().tan();
        let cx = w0 / 2.0;
        let cy = h0 / 2.0;
        let ux = (x - cx) * sx;
        let uy = (y - cy) * sy;
        let tx = ux + kx * uy;
        let ty = ky * ux + uy;
        let rad = self.rotation_deg.to_radians();
        let (cos, sin) = (rad.cos(), rad.sin());
        (
            tx * cos - ty * sin + cx * sx + self.offset_x,
            tx * sin + ty * cos + cy * sy + self.offset_y,
        )
    }

    /// The image's 4 corners (tl, tr, br, bl) in doc coordinates.
    #[must_use]
    pub fn doc_corners(&self, w0: f32, h0: f32) -> [(f32, f32); 4] {
        [(0.0, 0.0), (w0, 0.0), (w0, h0), (0.0, h0)].map(|(x, y)| self.local_to_doc(w0, h0, x, y))
    }

    /// Inverse of [`Self::local_to_doc`]: doc (canvas) pixels back to
    /// local image pixels (top-left origin). Used to commit brush
    /// strokes into moved/rotated/scaled layers at the cursor position
    /// (painting without it lands offset by the layer transform).
    /// `None` when degenerate (non-invertible shear/scale) — callers
    /// fall back to a translation-only mapping, never a silent offset.
    #[must_use]
    pub fn doc_to_local(&self, w0: f32, h0: f32, dx: f32, dy: f32) -> Option<(f32, f32)> {
        if !dx.is_finite() || !dy.is_finite() {
            return None;
        }
        let sx = self.scale_x;
        let sy = self.scale_y;
        if sx.abs() < 1e-6 || sy.abs() < 1e-6 {
            return None;
        }
        let kx = self.skew_x.to_radians().tan();
        let ky = self.skew_y.to_radians().tan();
        if !kx.is_finite() || !ky.is_finite() {
            return None;
        }
        let cx = w0 / 2.0;
        let cy = h0 / 2.0;
        // Undo offset, then inverse rotation.
        let px = dx - (cx * sx + self.offset_x);
        let py = dy - (cy * sy + self.offset_y);
        let rad = self.rotation_deg.to_radians();
        let (cos, sin) = (rad.cos(), rad.sin());
        let qx = px * cos + py * sin;
        let qy = -px * sin + py * cos;
        // Undo shear×scale: [[1, kx], [ky, 1]]^-1, then divide scales.
        let det = 1.0 - kx * ky;
        if det.abs() < 1e-6 {
            return None;
        }
        let ux = (qx - kx * qy) / det;
        let uy = (qy - ky * qx) / det;
        let (x, y) = (ux / sx + cx, uy / sy + cy);
        (x.is_finite() && y.is_finite()).then_some((x, y))
    }

    /// (m00, m01, m10, m11) — the shear×scale matrix, BEFORE rotation.
    /// Used both by rendering (compositing) and by any future
    /// corner/handle computation. ONLY place where `tan()` is applied.
    #[must_use]
    pub fn shear_scale_matrix(&self) -> (f32, f32, f32, f32) {
        let kx = self.skew_x.to_radians().tan();
        let ky = self.skew_y.to_radians().tan();
        (
            self.scale_x,
            kx * self.scale_y,
            ky * self.scale_x,
            self.scale_y,
        )
    }

    /// True when skew is active (the fast GPU path cannot
    /// display it: compositing will go through the CPU).
    #[must_use]
    pub fn has_skew(&self) -> bool {
        self.skew_x.abs() > 0.001 || self.skew_y.abs() > 0.001
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f32, f32), b: (f32, f32)) -> bool {
        (a.0 - b.0).abs() < 1e-3 && (a.1 - b.1).abs() < 1e-3
    }

    #[test]
    fn doc_to_local_roundtrips() {
        // Identity, translation, scale, rotation, skew: forward then
        // back lands on the starting pixel.
        let cases = [
            Transform2D::default(),
            Transform2D {
                offset_x: 120.0,
                offset_y: -45.0,
                ..Transform2D::default()
            },
            Transform2D {
                scale_x: 2.0,
                scale_y: 0.5,
                offset_x: 10.0,
                offset_y: 20.0,
                ..Transform2D::default()
            },
            Transform2D {
                rotation_deg: 30.0,
                offset_x: -15.0,
                offset_y: 40.0,
                ..Transform2D::default()
            },
            Transform2D {
                skew_x: 12.0,
                skew_y: -6.0,
                scale_x: 1.5,
                ..Transform2D::default()
            },
        ];
        for t in cases {
            for (x, y) in [(0.0, 0.0), (800.0, 600.0), (123.25, 456.75)] {
                let (dx, dy) = t.local_to_doc(800.0, 600.0, x, y);
                let back = t.doc_to_local(800.0, 600.0, dx, dy).expect("inversible");
                assert!(close(back, (x, y)), "{t:?} ({x},{y}) -> {back:?}");
            }
        }
    }

    #[test]
    fn is_finite_covers_all_fields() {
        assert!(Transform2D::default().is_finite());
        for t in [
            Transform2D {
                offset_x: f32::INFINITY,
                ..Transform2D::default()
            },
            Transform2D {
                rotation_deg: f32::NAN,
                ..Transform2D::default()
            },
            Transform2D {
                scale_y: f32::NAN,
                ..Transform2D::default()
            },
            Transform2D {
                skew_x: f32::INFINITY,
                ..Transform2D::default()
            },
        ] {
            assert!(!t.is_finite(), "{t:?}");
        }
    }

    #[test]
    fn doc_to_local_rejects_degenerate() {
        assert!(
            Transform2D::default()
                .doc_to_local(10.0, 10.0, f32::NAN, 0.0)
                .is_none()
        );
        let flat = Transform2D {
            scale_x: 0.0,
            ..Transform2D::default()
        };
        assert!(flat.doc_to_local(10.0, 10.0, 5.0, 5.0).is_none());
        let skewed = Transform2D {
            skew_x: 45.0,
            skew_y: 45.0,
            ..Transform2D::default()
        };
        // kx*ky = 1 → det = 0.
        assert!(skewed.doc_to_local(10.0, 10.0, 5.0, 5.0).is_none());
    }
}

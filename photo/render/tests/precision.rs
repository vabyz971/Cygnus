// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Export precision through the 16-bit integer cache (docs/decisions/0026):
//! exact round trips of opaque pixels in 8 and 16 bits, and the documented
//! limit of semi-transparent pixels, whose color is stored premultiplied.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build, setup};
use cygnus_photo_core::commands::{DocumentCreate, LayerCreate, PixelsWrite};
use cygnus_photo_core::{Depth, NodeId, PhotoCommand, Rect};
use cygnus_photo_render::{ExportBits, Image, Renderer};

/// A document of `width × height` pixels holding `samples` in one layer.
fn document(depth: Depth, width: u32, height: u32, samples: Vec<u16>) -> Vec<PhotoCommand> {
    vec![
        PhotoCommand::DocumentCreate(DocumentCreate {
            width,
            height,
            resolution: 72.0,
            depth,
            profile: Default::default(),
        }),
        PhotoCommand::LayerCreate(LayerCreate {
            name: "source".to_owned(),
            parent: None,
            index: None,
            kind: Default::default(),
        }),
        PhotoCommand::PixelsWrite(PixelsWrite {
            layer: NodeId(1),
            rect: Rect {
                x: 0,
                y: 0,
                width,
                height,
            },
            samples,
        }),
    ]
}

fn export(renderer: &Renderer, commands: Vec<PhotoCommand>, bits: ExportBits) -> Image {
    renderer.export(&build(commands), bits).unwrap()
}

/// 8-bit value of a 16-bit one, in integers: (v16 + 128) / 257.
fn to_8(v16: u16) -> u16 {
    ((u32::from(v16) + 128) / 257) as u16
}

#[test]
fn every_8_bit_value_round_trips_through_16_bits() {
    let Some((_, renderer)) = setup() else { return };
    // Row 0, opaque: (v, 255 - v, v, 255). Row 1: white at alpha v.
    let mut samples = Vec::new();
    for v in 0..256u16 {
        samples.extend([v, 255 - v, v, 255]);
    }
    for v in 0..256u16 {
        samples.extend([255, 255, 255, v]);
    }
    let commands = document(Depth::U8, 256, 2, samples.clone());
    let sixteen = export(&renderer, commands.clone(), ExportBits::Sixteen);
    let eight = export(&renderer, commands, ExportBits::Eight);
    for (i, source) in samples.iter().enumerate() {
        let (x, y) = ((i / 4) % 256, i / 1024);
        let v16 = sixteen.data[i];
        let v8 = eight.data[i];
        // A fully transparent pixel has no color: it exports as zero.
        let expected = if y == 1 && x == 0 { 0 } else { *source };
        assert_eq!(
            v16,
            expected * 257,
            "8 -> 16 at ({x}, {y}) channel {}",
            i % 4
        );
        assert_eq!(to_8(v16), expected, "16 -> 8 formula at ({x}, {y})");
        assert_eq!(v8, expected, "8-bit export at ({x}, {y}) channel {}", i % 4);
    }
}

#[test]
fn opaque_16_bit_values_export_exactly() {
    let Some((_, renderer)) = setup() else { return };
    // 256 × 256 pixels: every 16-bit value, in three channel orders.
    let mut samples = Vec::new();
    for i in 0..65536u32 {
        let j = i ^ 0x5555;
        samples.extend([i, 65535 - i, j, 65535].map(|v| v as u16));
    }
    let commands = document(Depth::U16, 256, 256, samples.clone());
    let sixteen = export(&renderer, commands.clone(), ExportBits::Sixteen);
    let eight = export(&renderer, commands, ExportBits::Eight);
    let wrong16 = sixteen
        .data
        .iter()
        .zip(&samples)
        .filter(|(a, b)| a != b)
        .count();
    let wrong8 = eight
        .data
        .iter()
        .zip(&samples)
        .filter(|(a, b)| **a != to_8(**b))
        .count();
    assert_eq!(wrong16, 0, "16-bit export differs from the source");
    assert_eq!(wrong8, 0, "8-bit export differs from (v16 + 128) / 257");
}

/// Largest error of the straight color of a pixel of alpha `alpha` (16-bit
/// units), in 16-bit units: the premultiplied color is rounded to half a
/// unit, which unpremultiplying divides by the alpha; the export rounds
/// again. One unit more covers the f32 arithmetic.
fn unpremultiply_bound(alpha: u16) -> f64 {
    0.5 * 65535.0 / f64::from(alpha) + 0.5 + 1.0
}

#[test]
fn transparency_gradient_error_stays_within_the_unpremultiply_bound() {
    let Some((_, renderer)) = setup() else { return };
    // Every alpha from 0 to 65535 under one color.
    let color = [52428u16, 19661, 3277];
    let mut samples = Vec::new();
    for alpha in 0..65536u32 {
        samples.extend([color[0], color[1], color[2], alpha as u16]);
    }
    let commands = document(Depth::U16, 256, 256, samples);
    let sixteen = export(&renderer, commands.clone(), ExportBits::Sixteen);
    let eight = export(&renderer, commands, ExportBits::Eight);
    // Largest error per alpha range, reported for docs/decisions/0026.
    let ranges = [(1u16, 256u16), (257, 2570), (2571, 32767), (32768, 65535)];
    let mut worst16 = [0u16; 4];
    let mut worst8 = [0u16; 4];
    for (i, pixel) in sixteen.data.as_chunks::<4>().0.iter().enumerate() {
        let alpha = i as u16;
        let pixel8 = &eight.data[i * 4..i * 4 + 4];
        assert_eq!(pixel[3], alpha, "alpha {alpha} is exported exactly");
        assert_eq!(pixel8[3], to_8(alpha), "8-bit alpha of {alpha}");
        if alpha == 0 {
            assert_eq!(*pixel, [0; 4], "transparent pixel has no color");
            continue;
        }
        let range = ranges
            .iter()
            .position(|(lo, hi)| (*lo..=*hi).contains(&alpha));
        for c in 0..3 {
            let error = pixel[c].abs_diff(color[c]);
            assert!(
                f64::from(error) <= unpremultiply_bound(alpha),
                "alpha {alpha}, channel {c}: error {error} above {}",
                unpremultiply_bound(alpha)
            );
            let error8 = pixel8[c].abs_diff(to_8(color[c]));
            // In 8 bits the error stays within one level as soon as the
            // pixel is visible in an 8-bit alpha (alpha >= 129, rounding
            // to 1 / 255).
            if alpha >= 129 {
                assert!(
                    error8 <= 1,
                    "alpha {alpha}, channel {c}: 8-bit error {error8}"
                );
            }
            if let Some(range) = range {
                worst16[range] = worst16[range].max(error);
                worst8[range] = worst8[range].max(error8);
            }
        }
    }
    for (range, (lo, hi)) in ranges.iter().enumerate() {
        println!(
            "alpha {lo}..={hi}: largest error {} (16 bits), {} (8 bits)",
            worst16[range], worst8[range]
        );
    }
}

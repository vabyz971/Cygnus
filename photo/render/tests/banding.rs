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

//! Banding test (docs/decisions/0011, condition of Q4): smooth 16-bit
//! gradients go through the 16-bit integer cache (docs/decisions/0026) and
//! the 16-bit export. The output
//! must stay monotonic, and no step between neighbors may exceed a
//! threshold, set from the first measurement and never loosened without
//! evidence.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build, export16, setup};
use cygnus_photo_core::commands::{DocumentCreate, LayerCreate, PixelsWrite};
use cygnus_photo_core::{Depth, NodeId, PhotoCommand, Rect};

/// Largest step allowed between two neighbors of a gradient whose input
/// step is 16 (full range) or 1 (dark range), in 16-bit units. Measured on
/// 2026-11-01 with the 16-bit integer cache, identical on lavapipe and an
/// NVIDIA RTX 3080 Ti: 16 and 1, the input steps themselves (no banding
/// added). With the f16 cache of J2 they were 31 and 3.
const FULL_RANGE_MAX_STEP: u16 = 16;
const DARK_RANGE_MAX_STEP: u16 = 1;

const WIDTH: u32 = 4096;

fn gradient(value: impl Fn(u32) -> u16) -> Vec<PhotoCommand> {
    let samples = (0..WIDTH)
        .flat_map(|x| {
            let v = value(x);
            [v, v, v, u16::MAX]
        })
        .collect();
    vec![
        PhotoCommand::DocumentCreate(DocumentCreate {
            width: WIDTH,
            height: 1,
            resolution: 72.0,
            depth: Depth::U16,
            profile: Default::default(),
        }),
        PhotoCommand::LayerCreate(LayerCreate {
            name: "gradient".to_owned(),
            parent: None,
            index: None,
            kind: Default::default(),
        }),
        PhotoCommand::PixelsWrite(PixelsWrite {
            layer: NodeId(1),
            rect: Rect {
                x: 0,
                y: 0,
                width: WIDTH,
                height: 1,
            },
            samples,
        }),
    ]
}

/// Monotonicity and largest step of the red channel of row 0.
fn measure(values: &[u16]) -> (bool, u16) {
    let monotonic = values.windows(2).all(|pair| pair[1] >= pair[0]);
    let max_step = values
        .windows(2)
        .map(|pair| pair[1].saturating_sub(pair[0]))
        .max()
        .unwrap_or(0);
    (monotonic, max_step)
}

fn check(name: &str, value: impl Fn(u32) -> u16, threshold: u16) {
    let Some((_, renderer)) = setup() else { return };
    let image = export16(&renderer, &build(gradient(value)));
    let red: Vec<u16> = (0..WIDTH).map(|x| image.pixel(x, 0)[0]).collect();
    let (monotonic, max_step) = measure(&red);
    println!("{name}: monotonic = {monotonic}, largest step = {max_step}");
    assert!(monotonic, "{name}: gradient is not monotonic");
    assert!(
        max_step <= threshold,
        "{name}: step {max_step} above {threshold}"
    );
}

#[test]
fn full_range_gradient_has_no_bands() {
    // Input step: 65535 / 4095 = 16.
    check(
        "full range",
        |x| (x * 16).min(65535) as u16,
        FULL_RANGE_MAX_STEP,
    );
}

#[test]
fn dark_gradient_has_no_bands() {
    // Darkest 4096 values, input step 1: the most sensitive range.
    check("dark range", |x| x as u16, DARK_RANGE_MAX_STEP);
}

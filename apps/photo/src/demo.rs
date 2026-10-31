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

//! Demonstration document shown at start-up until documents can be opened
//! (milestone J6): a procedural landscape in five pixel layers, built with
//! commands through the bus like any other change.

use cygnus_photo_core::commands::{DocumentCreate, LayerCreate, LayerUpdate, PixelsWrite};
use cygnus_photo_core::{Depth, NodeId, PhotoCommand, Rect};

/// Document size, in pixels.
pub const WIDTH: u32 = 1920;
/// Document size, in pixels.
pub const HEIGHT: u32 = 1280;
const HORIZON: f32 = 1000.0;

type Rgba = [f32; 4];

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// A layer filled by `color(x, y)` (straight sRGB, 0..1) over `rect`.
fn layer(id: u64, name: &str, rect: Rect, color: impl Fn(f32, f32) -> Rgba) -> Vec<PhotoCommand> {
    let mut samples = Vec::with_capacity((rect.width * rect.height * 4) as usize);
    for y in rect.y..rect.y + rect.height {
        for x in rect.x..rect.x + rect.width {
            let rgba = color(x as f32 + 0.5, y as f32 + 0.5);
            samples.extend(rgba.map(|c| (c.clamp(0.0, 1.0) * 65535.0).round() as u16));
        }
    }
    vec![
        PhotoCommand::LayerCreate(LayerCreate {
            name: name.to_owned(),
            parent: None,
            index: None,
        }),
        PhotoCommand::PixelsWrite(PixelsWrite {
            layer: NodeId(id),
            rect,
            samples,
        }),
    ]
}

fn opacity(id: u64, opacity: f32) -> PhotoCommand {
    PhotoCommand::LayerUpdate(LayerUpdate {
        opacity: Some(opacity),
        ..LayerUpdate::new(NodeId(id))
    })
}

/// Height of the mountain ridge at `x`, in pixels from the top.
fn ridge(x: f32) -> f32 {
    let u = x / WIDTH as f32;
    640.0
        - 170.0 * (u * 7.0 + 0.6).sin().abs() * (1.0 - u * 0.3)
        - 70.0 * (u * 19.0 + 1.3).sin()
        - 25.0 * (u * 53.0).sin()
}

/// Commands building the demonstration document. Layer names are user
/// content, like a file name; they are not interface texts.
pub fn commands(names: [&str; 5]) -> Vec<PhotoCommand> {
    let full = Rect::of_size(WIDTH, HEIGHT);
    let mut commands = vec![PhotoCommand::DocumentCreate(DocumentCreate {
        width: WIDTH,
        height: HEIGHT,
        resolution: 300.0,
        depth: Depth::U16,
        profile: Default::default(),
    })];

    // 1. Sky and ground: opaque gradient.
    commands.extend(layer(1, names[0], full, |_, y| {
        let rgb = if y < HORIZON {
            mix(
                [0.09, 0.14, 0.33],
                [0.98, 0.68, 0.47],
                (y / HORIZON).powf(1.6),
            )
        } else {
            mix(
                [0.20, 0.26, 0.36],
                [0.07, 0.10, 0.16],
                (y - HORIZON) / 280.0,
            )
        };
        [rgb[0], rgb[1], rgb[2], 1.0]
    }));

    // 2. Sun with a soft glow.
    let (cx, cy) = (1340.0, 430.0);
    commands.extend(layer(
        2,
        names[1],
        Rect {
            x: 1090,
            y: 180,
            width: 500,
            height: 500,
        },
        move |x, y| {
            let d = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt();
            let disc = 1.0 - smoothstep(118.0, 122.0, d);
            let glow = 0.55 * (1.0 - smoothstep(120.0, 250.0, d));
            [1.0, 0.88, 0.58, disc.max(glow)]
        },
    ));

    // 3. Mountains, anti-aliased ridge.
    let top = 380;
    commands.extend(layer(
        3,
        names[2],
        Rect {
            x: 0,
            y: top,
            width: WIDTH,
            height: HORIZON as u32 - top,
        },
        |x, y| {
            let coverage = (y - ridge(x) + 0.5).clamp(0.0, 1.0);
            let shade = mix([0.24, 0.24, 0.36], [0.11, 0.13, 0.21], (y - 400.0) / 600.0);
            [shade[0], shade[1], shade[2], coverage]
        },
    ));
    commands.push(opacity(3, 0.95));

    // 4. Lake.
    commands.extend(layer(
        4,
        names[3],
        Rect {
            x: 0,
            y: HORIZON as u32,
            width: WIDTH,
            height: HEIGHT - HORIZON as u32,
        },
        |_, y| {
            let rgb = mix(
                [0.16, 0.30, 0.42],
                [0.05, 0.12, 0.19],
                (y - HORIZON) / 280.0,
            );
            [rgb[0], rgb[1], rgb[2], 1.0]
        },
    ));
    commands.push(opacity(4, 0.85));

    // 5. Mist over the horizon.
    commands.extend(layer(
        5,
        names[4],
        Rect {
            x: 0,
            y: 880,
            width: WIDTH,
            height: 240,
        },
        |_, y| {
            let t = (y - HORIZON) / 60.0;
            [0.95, 0.93, 0.97, 0.45 * (-t * t).exp()]
        },
    ));
    commands.push(opacity(5, 0.5));
    commands
}

#[cfg(test)]
mod tests {
    use super::*;
    use cygnus_commands::Source;
    use cygnus_photo_core::script::{SCRIPT_VERSION, Script};

    #[test]
    fn demo_document_builds() {
        let script = Script {
            version: SCRIPT_VERSION,
            commands: commands(["a", "b", "c", "d", "e"]),
        };
        let document = script.build(Source::Test).unwrap();
        assert_eq!((document.width, document.height), (WIDTH, HEIGHT));
        assert_eq!(document.children(document.root).len(), 5);
        document.validate().unwrap();
    }
}

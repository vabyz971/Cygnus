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

//! Generates the image files of the photo scenario (docs/decisions/0027):
//! a 16-bit PNG and an 8-bit JPEG, procedural, under the license of the
//! project, so that no third-party photo is stored in the repository.
//!
//! `cargo run -p cygnus-photo-io --example fixtures -- <folder>` writes
//! `paysage-16.png` and `texture-8.jpg`; the output is deterministic.

use std::error::Error;
use std::path::PathBuf;

const WIDTH: u32 = 480;
const HEIGHT: u32 = 320;

fn smooth(edge0: f64, edge1: f64, x: f64) -> f64 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f64; 3], b: [f64; 3], t: f64) -> [f64; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * t)
}

/// A landscape: sky gradient, sun, two hills, in 16 bits.
fn landscape(x: u32, y: u32) -> [f64; 3] {
    let (u, v) = (
        f64::from(x) / f64::from(WIDTH),
        f64::from(y) / f64::from(HEIGHT),
    );
    let mut color = mix([0.16, 0.32, 0.68], [0.93, 0.80, 0.62], smooth(0.0, 0.7, v));
    let sun = ((u - 0.72).powi(2) + (v - 0.28).powi(2)).sqrt();
    color = mix(color, [1.0, 0.95, 0.78], 1.0 - smooth(0.06, 0.075, sun));
    let far = 0.58 + 0.06 * (u * 9.0).sin() + 0.03 * (u * 23.0).cos();
    color = mix(
        color,
        mix([0.35, 0.45, 0.42], [0.25, 0.34, 0.30], v),
        smooth(far - 0.004, far + 0.004, v),
    );
    let near = 0.74 + 0.08 * (u * 4.0 + 1.0).sin();
    color = mix(
        color,
        mix([0.30, 0.52, 0.16], [0.10, 0.24, 0.06], v),
        smooth(near - 0.003, near + 0.003, v),
    );
    color
}

/// A texture: rings and diagonal stripes, in 8 bits.
fn texture(x: u32, y: u32) -> [f64; 3] {
    let (u, v) = (
        f64::from(x) / f64::from(WIDTH),
        f64::from(y) / f64::from(HEIGHT),
    );
    let rings = (((u - 0.3).powi(2) + (v - 0.6).powi(2)).sqrt() * 40.0).sin() * 0.5 + 0.5;
    let stripes = ((u + v) * 30.0).sin() * 0.5 + 0.5;
    [
        0.2 + 0.6 * rings,
        0.15 + 0.5 * stripes * rings,
        0.55 - 0.4 * stripes,
    ]
}

fn main() -> Result<(), Box<dyn Error>> {
    let folder = PathBuf::from(std::env::args().nth(1).ok_or("expects an output folder")?);
    std::fs::create_dir_all(&folder)?;

    let mut data = Vec::with_capacity((WIDTH * HEIGHT * 8) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            for value in landscape(x, y) {
                let v = (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
                data.extend(v.to_be_bytes());
            }
            data.extend(u16::MAX.to_be_bytes());
        }
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, WIDTH, HEIGHT);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Sixteen);
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&data)?;
    }
    std::fs::write(folder.join("paysage-16.png"), png_bytes)?;

    let mut rgb = Vec::with_capacity((WIDTH * HEIGHT * 3) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            rgb.extend(texture(x, y).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
        }
    }
    let mut jpeg_bytes = Vec::new();
    jpeg_encoder::Encoder::new(&mut jpeg_bytes, 90).encode(
        &rgb,
        WIDTH as u16,
        HEIGHT as u16,
        jpeg_encoder::ColorType::Rgb,
    )?;
    std::fs::write(folder.join("texture-8.jpg"), jpeg_bytes)?;
    println!(
        "wrote paysage-16.png and texture-8.jpg in {}",
        folder.display()
    );
    Ok(())
}

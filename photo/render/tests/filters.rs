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

//! Neighbourhood filters (docs/decisions/0023, step B10): Gaussian blur and
//! unsharp mask, against a reference computed here in f64 with the same
//! conventions (encoded values, premultiplied alpha, separable kernel of
//! three standard deviations, document edge extended, each pass rounded to
//! 16 bits), across tile borders and with radii reaching two tiles away.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{build_json, draw_on, export16, setup};
use cygnus_commands::{ChangeSet, CommandBus, Source};
use cygnus_photo_core::commands::PixelsFill;
use cygnus_photo_core::{Document, NodeId, PhotoChanges, PhotoCommand, Rect};
use cygnus_photo_render::{Canvas, CanvasView, ExportBits, Image};

/// Largest gap allowed with the reference, in 16-bit units: f32 sums of
/// up to 601 taps against f64, after two roundings.
const TOLERANCE: u16 = 3;

/// A premultiplied image in f64, row by row.
struct Plane {
    width: usize,
    height: usize,
    data: Vec<[f64; 4]>,
}

impl Plane {
    /// The 16-bit export of an opaque document as a plane.
    fn from_image(image: &Image) -> Self {
        let (width, height) = (image.width as usize, image.height as usize);
        let data = (0..width * height)
            .map(|i| {
                let p = image.pixel((i % width) as u32, (i / width) as u32);
                let alpha = f64::from(p[3]) / 65535.0;
                [
                    f64::from(p[0]) / 65535.0 * alpha,
                    f64::from(p[1]) / 65535.0 * alpha,
                    f64::from(p[2]) / 65535.0 * alpha,
                    alpha,
                ]
            })
            .collect();
        Self {
            width,
            height,
            data,
        }
    }

    fn at(&self, x: i64, y: i64) -> [f64; 4] {
        let x = x.clamp(0, self.width as i64 - 1) as usize;
        let y = y.clamp(0, self.height as i64 - 1) as usize;
        self.data[y * self.width + x]
    }

    /// Rounded to 16 bits, as the stage tiles store it.
    fn rounded(mut self) -> Self {
        for pixel in &mut self.data {
            for c in pixel.iter_mut() {
                *c = (c.clamp(0.0, 1.0) * 65535.0).round() / 65535.0;
            }
        }
        self
    }

    fn pass(&self, weights: &[f64], horizontal: bool) -> Self {
        let r = weights.len() as i64 - 1;
        let mut data = Vec::with_capacity(self.data.len());
        for y in 0..self.height as i64 {
            for x in 0..self.width as i64 {
                let mut sum = [0.0; 4];
                for i in -r..=r {
                    let (sx, sy) = if horizontal { (x + i, y) } else { (x, y + i) };
                    let w = weights[i.unsigned_abs() as usize];
                    let p = self.at(sx, sy);
                    for c in 0..4 {
                        sum[c] += w * p[c];
                    }
                }
                data.push(sum);
            }
        }
        Self {
            width: self.width,
            height: self.height,
            data,
        }
        .rounded()
    }

    fn blur(&self, sigma: f64) -> Self {
        let r = (3.0 * sigma).ceil().max(1.0) as usize;
        let raw: Vec<f64> = (0..=r)
            .map(|i| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp())
            .collect();
        let total = raw[0] + 2.0 * raw[1..].iter().sum::<f64>();
        let weights: Vec<f64> = raw.iter().map(|w| w / total).collect();
        self.pass(&weights, true).pass(&weights, false)
    }

    /// Straight 16-bit values of pixel (x, y).
    fn export(&self, x: usize, y: usize) -> [u16; 4] {
        let p = self.data[y * self.width + x];
        let alpha = p[3];
        let mut out = [0u16; 4];
        if alpha > 0.0 {
            for c in 0..3 {
                out[c] = ((p[c] / alpha).clamp(0.0, 1.0) * 65535.0).round() as u16;
            }
        }
        out[3] = (alpha * 65535.0).round() as u16;
        out
    }
}

fn assert_matches(image: &Image, expected: &Plane, what: &str) {
    let mut worst = (0u16, 0, 0);
    for y in 0..expected.height {
        for x in 0..expected.width {
            let got = image.pixel(x as u32, y as u32);
            let want = expected.export(x, y);
            for c in 0..4 {
                let gap = got[c].abs_diff(want[c]);
                if gap > worst.0 {
                    worst = (gap, x, y);
                }
            }
        }
    }
    assert!(
        worst.0 <= TOLERANCE,
        "{what}: {} units at ({}, {}): got {:?}, expected {:?}",
        worst.0,
        worst.1,
        worst.2,
        image.pixel(worst.1 as u32, worst.2 as u32),
        expected.export(worst.1, worst.2)
    );
}

/// An opaque 600 × 300 document (3 × 2 tiles): flat areas, sharp edges and
/// thin lines crossing the tile borders.
fn scene(extra: &str) -> Document {
    build_json(&format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 600, "height": 300, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "fond"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": 600, "height": 300}}, "color": [0.15, 0.2, 0.3, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 200, "y": 40, "width": 120, "height": 230}}, "color": [0.9, 0.75, 0.2, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 255, "width": 600, "height": 2}}, "color": [1.0, 1.0, 1.0, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 511, "y": 0, "width": 3, "height": 300}}, "color": [0.0, 0.0, 0.0, 1.0]}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 590, "y": 290, "width": 10, "height": 10}}, "color": [0.1, 0.9, 0.3, 1.0]}}}}
        {extra}
    ]"#
    ))
}

fn blur_layer(radius: f64) -> String {
    format!(
        r#", {{"command": "layer.create", "params": {{"name": "flou", "kind": {{"type": "filter", "filter": {{"kind": "gaussian_blur", "radius": {radius}}}}}}}}}"#
    )
}

#[test]
fn a_gaussian_blur_matches_the_reference_across_tiles() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let original = Plane::from_image(&export16(&renderer, &scene("")));
    for sigma in [0.5, 3.0, 40.0] {
        let image = export16(&renderer, &scene(&blur_layer(sigma)));
        assert_matches(&image, &original.blur(sigma), &format!("sigma {sigma}"));
    }
}

#[test]
fn a_blur_reaching_two_tiles_away_matches_the_reference() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    // 3 × 90 = 270 pixels: more than one tile of margin.
    let original = Plane::from_image(&export16(&renderer, &scene("")));
    let image = export16(&renderer, &scene(&blur_layer(90.0)));
    assert_matches(&image, &original.blur(90.0), "sigma 90");
}

#[test]
fn the_unsharp_mask_sharpens_beyond_its_threshold_only() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let (sigma, amount, threshold) = (2.0, 1.5, 6.0);
    let original = Plane::from_image(&export16(&renderer, &scene(""))).rounded();
    let image = export16(
        &renderer,
        &scene(&format!(
            r#", {{"command": "layer.create", "params": {{"name": "net", "kind": {{"type": "filter", "filter": {{"kind": "unsharp_mask", "radius": {sigma}, "amount": {amount}, "threshold": {threshold}}}}}}}}}"#
        )),
    );
    let blurred = original.blur(sigma);
    let mut expected = Plane {
        width: original.width,
        height: original.height,
        data: Vec::with_capacity(original.data.len()),
    };
    for (source, blur) in original.data.iter().zip(&blurred.data) {
        let mut out = [0.0; 4];
        for c in 0..4 {
            let difference = source[c] - blur[c];
            out[c] = if difference.abs() <= threshold / 255.0 {
                source[c]
            } else {
                source[c] + amount * difference
            };
        }
        let alpha = out[3].clamp(0.0, 1.0);
        for c in out.iter_mut().take(3) {
            *c = c.clamp(0.0, alpha);
        }
        out[3] = alpha;
        expected.data.push(out);
    }
    let expected = expected.rounded();
    assert_matches(&image, &expected, "unsharp mask");
    // Flat areas are left alone: the threshold holds.
    assert_eq!(
        image.pixel(100, 120),
        export16(&renderer, &scene("")).pixel(100, 120)
    );
}

#[test]
fn opacity_mixes_the_filtered_result_with_what_is_below() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    let sigma = 3.0;
    let original = Plane::from_image(&export16(&renderer, &scene(""))).rounded();
    let blurred = original.blur(sigma);
    let image = export16(
        &renderer,
        &scene(&format!(
            r#"{}, {{"command": "layer.update", "params": {{"id": 2, "opacity": 0.5}}}}"#,
            blur_layer(sigma)
        )),
    );
    let mixed = Plane {
        width: original.width,
        height: original.height,
        data: original
            .data
            .iter()
            .zip(&blurred.data)
            .map(|(a, b)| [0, 1, 2, 3].map(|c| a[c] + (b[c] - a[c]) * 0.5))
            .collect(),
    }
    .rounded();
    assert_matches(&image, &mixed, "opacity 50 %");
}

#[test]
fn a_filter_in_an_isolated_group_blurs_the_group_only() {
    let Some((_gpu, renderer)) = setup() else {
        return;
    };
    // Below: the opaque scene. In an isolated group: a translucent square
    // and a blur, which blurs the square and its transparency only.
    let square = r#", {"command": "layer.create", "params": {"name": "carre"}},
        {"command": "pixels.fill", "params": {"layer": 2, "rect": {"x": 230, "y": 200, "width": 60, "height": 60}, "color": [0.8, 0.1, 0.6, 0.8]}}"#;
    let document = build_json(&format!(
        r#"[
        {{"command": "document.create", "params": {{"width": 600, "height": 300, "depth": "u16"}}}},
        {{"command": "layer.create", "params": {{"name": "carre"}}}},
        {{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 230, "y": 200, "width": 60, "height": 60}}, "color": [0.8, 0.1, 0.6, 0.8]}}}}
        {blur}
    ]"#,
        blur = blur_layer(4.0)
    ));
    let square_only = build_json(
        r#"[
        {"command": "document.create", "params": {"width": 600, "height": 300, "depth": "u16"}},
        {"command": "layer.create", "params": {"name": "carre"}},
        {"command": "pixels.fill", "params": {"layer": 1, "rect": {"x": 230, "y": 200, "width": 60, "height": 60}, "color": [0.8, 0.1, 0.6, 0.8]}}
    ]"#,
    );
    let blurred_square = Plane::from_image(&export16(&renderer, &square_only))
        .rounded()
        .blur(4.0);
    // Without anything below, the blurred square alone.
    assert_matches(&export16(&renderer, &document), &blurred_square, "alone");

    // In a group over the scene: the blurred square over the sharp scene.
    let grouped = scene(&format!(
        r#"{square}{blur}, {{"command": "layer.group", "params": {{"ids": [2, 3], "isolation": "isolated"}}}}"#,
        blur = blur_layer(4.0)
    ));
    let below = Plane::from_image(&export16(&renderer, &scene(""))).rounded();
    let over = Plane {
        width: below.width,
        height: below.height,
        data: blurred_square
            .data
            .iter()
            .zip(&below.data)
            .map(|(s, b)| [0, 1, 2, 3].map(|c| s[c] + b[c] * (1.0 - s[3])))
            .collect(),
    }
    .rounded();
    assert_matches(&export16(&renderer, &grouped), &over, "in a group");
}

fn view() -> CanvasView {
    CanvasView {
        origin: [0.0, 0.0],
        scale: 1.0,
        viewport: [0.0, 0.0, 600.0, 300.0],
        background: [0.0; 3],
        checker: [[0.5; 3], [0.4; 3]],
        checker_size: 8.0,
    }
}

#[test]
fn the_canvas_shows_the_export_and_follows_changes_across_the_margin() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let mut document = scene(&blur_layer(30.0));
    let mut canvas = Canvas::with_capacity(&renderer, wgpu::TextureFormat::Rgba8Unorm, 64).unwrap();
    let shown = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &PhotoChanges::everything(),
        (600, 300),
        view(),
    );
    let export: Vec<u8> = renderer
        .export(&document, ExportBits::Eight)
        .unwrap()
        .data
        .iter()
        .map(|v| *v as u8)
        .collect();
    assert_eq!(shown, export, "the canvas differs from the export");

    // A change in the last tile of the first row reaches the first tile
    // through the blur: the canvas follows it like a fresh export.
    let mut bus = CommandBus::new();
    let cursor = bus.revision();
    bus.execute(
        &mut document,
        PhotoCommand::PixelsFill(PixelsFill {
            layer: NodeId(1),
            rect: Rect {
                x: 512,
                y: 0,
                width: 20,
                height: 255,
            },
            color: [1.0, 0.0, 0.0, 1.0],
        }),
        Source::Test,
    )
    .unwrap();
    let changes = bus.changes_since(cursor).changes;
    let shown = draw_on(
        &gpu,
        &renderer,
        &mut canvas,
        &document,
        &changes,
        (600, 300),
        view(),
    );
    let export: Vec<u8> = renderer
        .export(&document, ExportBits::Eight)
        .unwrap()
        .data
        .iter()
        .map(|v| *v as u8)
        .collect();
    assert_eq!(
        shown, export,
        "the canvas missed a change across the margin"
    );
}

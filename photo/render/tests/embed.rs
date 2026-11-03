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

//! Embedded objects and `layer.merge_down` on the GPU (docs/decisions/0020
//! and 0021): an embedded object renders like a pixel layer holding the
//! same pixels, and merging two layers leaves the image unchanged.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::{TOLERANCE_UNITS, compare, export16, setup};
use cygnus_commands::{CommandBus, Source as Origin};
use cygnus_photo_core::pixels::{PixelStore, TileFormat};
use cygnus_photo_core::resolve::{Import, Resolvers, resolve_and_execute};
use cygnus_photo_core::{Depth, Document, NodeId, PhotoCommand, Rect, Source, SourceId};
use cygnus_photo_render::{ExportBits, Image, Renderer};

/// Pseudo-random straight RGBA samples of a `width` × `height` image of
/// `depth`, with varied alpha.
fn samples(width: u32, height: u32, depth: Depth, seed: u32) -> Vec<u16> {
    let max = match depth {
        Depth::U8 => 255u32,
        Depth::U16 => 65535,
    };
    let mut state = seed | 1;
    let mut next = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        (state % (max + 1)) as u16
    };
    (0..width * height)
        .flat_map(|i| {
            let alpha = if i % 3 == 0 { max as u16 } else { next() };
            [next(), next(), next(), alpha]
        })
        .collect()
}

/// Imports a source made of given samples, whatever the path.
struct Samples {
    width: u32,
    height: u32,
    depth: Depth,
    samples: Vec<u16>,
}

impl Import for Samples {
    fn import(&self, path: &str) -> Result<(SourceId, Source), String> {
        let mut store = PixelStore::default();
        let width = self.width as usize;
        store.paint(
            NodeId(0),
            Rect {
                x: 0,
                y: 0,
                width: self.width,
                height: self.height,
            },
            TileFormat::rgba(self.depth),
            |x, y| {
                let i = (y as usize * width + x as usize) * 4;
                [
                    self.samples[i],
                    self.samples[i + 1],
                    self.samples[i + 2],
                    self.samples[i + 3],
                ]
            },
        );
        Ok((
            SourceId(format!("test-{path}")),
            Source {
                name: path.to_owned(),
                width: self.width,
                height: self.height,
                depth: self.depth,
                tiles: store.take_layer(NodeId(0)),
            },
        ))
    }
}

fn command(json: &str) -> PhotoCommand {
    serde_json::from_str(json).unwrap_or_else(|error| panic!("{json}: {error}"))
}

/// Runs `commands` through the single resolution path.
fn run(commands: &[String], resolvers: &Resolvers<'_>) -> Document {
    let mut bus = CommandBus::new();
    let mut document = Document::default();
    for json in commands {
        resolve_and_execute(
            &mut bus,
            &mut document,
            command(json),
            Origin::Test,
            resolvers,
        )
        .unwrap_or_else(|error| panic!("{json}: {error}"));
    }
    document
}

fn create(width: u32, height: u32, depth: &str) -> String {
    format!(
        r#"{{"command": "document.create", "params": {{"width": {width}, "height": {height}, "depth": "{depth}"}}}}"#
    )
}

const BACKDROP: &str = r#"{"command": "layer.create", "params": {"name": "fond"}}"#;

fn backdrop_fill(width: u32, height: u32) -> String {
    format!(
        r#"{{"command": "pixels.fill", "params": {{"layer": 1, "rect": {{"x": 0, "y": 0, "width": {width}, "height": {height}}}, "color": [0.31, 0.47, 0.69, 1.0]}}}}"#
    )
}

/// The same pixels as an embedded object and as a pixel layer, over an
/// opaque backdrop, for every pairing of document and source depths and
/// a source larger and smaller than the document.
#[test]
fn an_embedded_object_renders_like_a_pixel_layer_of_the_same_pixels() {
    let Some((_, renderer)) = setup() else { return };
    let cases = [
        ("u16", Depth::U16, (300, 280), (260, 270)),
        ("u16", Depth::U8, (200, 100), (300, 300)),
        ("u8", Depth::U16, (300, 280), (260, 270)),
        ("u8", Depth::U8, (520, 300), (300, 260)),
    ];
    for (doc_depth, source_depth, (sw, sh), (dw, dh)) in cases {
        let what = format!("{doc_depth} document, {source_depth:?} source {sw} × {sh}");
        let data = samples(sw, sh, source_depth, sw * 7 + sh);
        let importer = Samples {
            width: sw,
            height: sh,
            depth: source_depth,
            samples: data.clone(),
        };
        let resolvers = Resolvers {
            import: Some(&importer),
            merge: None,
        };
        let embedded = run(
            &[
                create(dw, dh, doc_depth),
                BACKDROP.to_owned(),
                backdrop_fill(dw, dh),
                r#"{"command": "source.import", "params": {"path": "image"}}"#.to_owned(),
                r#"{"command": "layer.update", "params": {"id": 2, "opacity": 0.8}}"#.to_owned(),
            ],
            &resolvers,
        );
        // The pixel layer holds the same values, cropped to the document
        // and in its depth (8-bit sources widened × 257, 16-bit sources
        // written to an 8-bit document are rounded the same way by the
        // pixel store as by the upload of the embedded tiles: compare in
        // the depth of the source).
        let mut pixel = run(
            &[
                create(dw, dh, doc_depth),
                BACKDROP.to_owned(),
                backdrop_fill(dw, dh),
                r#"{"command": "layer.create", "params": {"name": "calque"}}"#.to_owned(),
                r#"{"command": "layer.update", "params": {"id": 2, "opacity": 0.8}}"#.to_owned(),
            ],
            &Resolvers::NONE,
        );
        let (w, h) = (sw.min(dw), sh.min(dh));
        pixel.pixels.paint(
            NodeId(2),
            Rect {
                x: 0,
                y: 0,
                width: w,
                height: h,
            },
            TileFormat::rgba(source_depth),
            |x, y| {
                let i = ((y * sw + x) * 4) as usize;
                [data[i], data[i + 1], data[i + 2], data[i + 3]]
            },
        );
        let expected = export16(&renderer, &pixel);
        let actual = export16(&renderer, &embedded);
        let comparison = compare(&expected, &actual);
        assert_eq!(comparison.differing, 0, "{what}: {comparison:?}");
    }
}

fn layer(name: &str) -> String {
    format!(r#"{{"command": "layer.create", "params": {{"name": "{name}"}}}}"#)
}

fn fill(layer: u64, x: u32, y: u32, w: u32, h: u32, color: &str) -> String {
    format!(
        r#"{{"command": "pixels.fill", "params": {{"layer": {layer}, "rect": {{"x": {x}, "y": {y}, "width": {w}, "height": {h}}}, "color": {color}}}}}"#
    )
}

fn update(id: u64, params: &str) -> String {
    format!(r#"{{"command": "layer.update", "params": {{"id": {id}, {params}}}}}"#)
}

fn merge(id: u64) -> String {
    format!(r#"{{"command": "layer.merge_down", "params": {{"id": {id}}}}}"#)
}

fn export(renderer: &Renderer, document: &Document) -> Image {
    let bits = match document.depth {
        Depth::U8 => ExportBits::Eight,
        Depth::U16 => ExportBits::Sixteen,
    };
    renderer.export(document, bits).unwrap()
}

/// Merging leaves the image unchanged: in 16 bits within the tolerance of
/// one adapter (the merged pixels are rounded once more), in 8 bits within
/// one level.
#[test]
fn merge_down_leaves_the_image_unchanged() {
    let Some((_, renderer)) = setup() else { return };
    // Layers: 1 opaque backdrop, 2 below, 3 upper; 300 × 200, two tiles.
    let scene = |depth: &str, extra: &[String]| -> Vec<String> {
        let mut commands = vec![
            create(300, 200, depth),
            layer("fond"),
            fill(1, 0, 0, 300, 200, "[0.83, 0.71, 0.52, 1.0]"),
            layer("dessous"),
            fill(2, 20, 20, 240, 150, "[0.12, 0.43, 0.77, 0.66]"),
            layer("dessus"),
            fill(3, 120, 60, 170, 130, "[0.91, 0.27, 0.18, 0.58]"),
        ];
        commands.extend(extra.iter().cloned());
        commands
    };
    let mask = |node: u64, mask: u64| {
        vec![
            format!(r#"{{"command": "mask.create", "params": {{"node": {node}}}}}"#),
            format!(
                r#"{{"command": "mask.gradient", "params": {{"mask": {mask}, "start": [0, 0], "end": [300, 0], "start_value": 0.1, "end_value": 1.0}}}}"#
            ),
        ]
    };
    let cases: Vec<(&str, &str, Vec<String>)> = vec![
        ("plain", "u16", vec![]),
        (
            "opacities and masks",
            "u16",
            [
                vec![
                    update(2, r#""opacity": 0.7"#),
                    update(3, r#""opacity": 0.45"#),
                ],
                mask(2, 4),
                mask(3, 5),
            ]
            .concat(),
        ),
        (
            "clipped onto its base",
            "u16",
            vec![update(3, r#""clip": true"#)],
        ),
        (
            "two clipped layers",
            "u16",
            vec![
                layer("sommet"),
                fill(4, 0, 100, 300, 100, "[0.2, 0.9, 0.3, 0.5]"),
                update(3, r#""clip": true"#),
                update(4, r#""clip": true"#),
            ],
        ),
        ("8 bits", "u8", vec![update(3, r#""opacity": 0.6"#)]),
    ];
    let resolvers = Resolvers {
        import: None,
        merge: Some(&renderer),
    };
    for (what, depth, extra) in cases {
        let target = if what == "two clipped layers" { 4 } else { 3 };
        let before = run(&scene(depth, &extra), &Resolvers::NONE);
        let mut commands = scene(depth, &extra);
        commands.push(merge(target));
        let after = run(&commands, &resolvers);
        assert!(after.node(NodeId(target)).is_none(), "{what}: not merged");
        let (a, b) = (export(&renderer, &before), export(&renderer, &after));
        let max = a
            .data
            .iter()
            .zip(&b.data)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0);
        let tolerance = if depth == "u8" { 1 } else { TOLERANCE_UNITS };
        assert!(max <= tolerance, "{what}: largest difference {max}");
    }
}

/// An embedded object merged with a pixel layer becomes a pixel layer
/// holding the composed image.
#[test]
fn an_embedded_object_can_be_merged_down() {
    let Some((_, renderer)) = setup() else { return };
    let importer = Samples {
        width: 64,
        height: 64,
        depth: Depth::U16,
        samples: samples(64, 64, Depth::U16, 99),
    };
    let resolvers = Resolvers {
        import: Some(&importer),
        merge: Some(&renderer),
    };
    let commands = vec![
        create(64, 64, "u16"),
        layer("fond"),
        fill(1, 0, 0, 64, 64, "[0.4, 0.4, 0.4, 1.0]"),
        r#"{"command": "source.import", "params": {"path": "image"}}"#.to_owned(),
    ];
    let before = run(&commands, &resolvers);
    let mut merged = commands.clone();
    merged.push(merge(2));
    let after = run(&merged, &resolvers);
    assert!(matches!(
        after.node(NodeId(1)).unwrap().kind,
        cygnus_photo_core::NodeKind::Pixel { .. }
    ));
    let comparison = compare(&export16(&renderer, &before), &export16(&renderer, &after));
    assert!(comparison.max_units <= TOLERANCE_UNITS, "{comparison:?}");
}

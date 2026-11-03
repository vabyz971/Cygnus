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

//! Decoding of `source.import` (docs/decisions/0020 and 0027): PNG 8 and 16
//! bits in every color type, JPEG; invalid and oversized files are refused
//! with an error, never a crash.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use cygnus_photo_core::resolve::Import;
use cygnus_photo_core::{Depth, Source};
use cygnus_photo_io::{FileImporter, IoError, decode};

fn png(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
) -> Vec<u8> {
    png_with(width, height, color, depth, data, |_| {})
}

fn png_with(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    data: &[u8],
    setup: impl FnOnce(&mut png::Encoder<'_, &mut Vec<u8>>),
) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, width, height);
        encoder.set_color(color);
        encoder.set_depth(depth);
        setup(&mut encoder);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(data).unwrap();
    }
    bytes
}

fn pixel(source: &Source, x: u32, y: u32) -> [u16; 4] {
    let coord = cygnus_photo_core::pixels::TileCoord {
        tx: x / 256,
        ty: y / 256,
    };
    source
        .tiles
        .get(&coord)
        .map_or([0; 4], |tile| tile.get(x % 256, y % 256))
}

#[test]
fn png_16_bit_rgba_keeps_every_value() {
    let values: [u16; 8] = [0, 1, 32768, 65535, 1000, 2000, 3000, 4000];
    let data: Vec<u8> = values.iter().flat_map(|v| v.to_be_bytes()).collect();
    let (_, source) = decode(
        &png(2, 1, png::ColorType::Rgba, png::BitDepth::Sixteen, &data),
        "a.png",
    )
    .unwrap();
    assert_eq!(
        (source.width, source.height, source.depth),
        (2, 1, Depth::U16)
    );
    assert_eq!(pixel(&source, 0, 0), [0, 1, 32768, 65535]);
    assert_eq!(pixel(&source, 1, 0), [1000, 2000, 3000, 4000]);
    assert_eq!(source.name, "a.png");
}

#[test]
fn png_8_bit_color_types_become_rgba() {
    // RGB, gray, gray with alpha, palette with transparency.
    let rgb = png(
        1,
        1,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &[10, 20, 30],
    );
    let gray = png(1, 1, png::ColorType::Grayscale, png::BitDepth::Eight, &[77]);
    let gray_alpha = png(
        1,
        1,
        png::ColorType::GrayscaleAlpha,
        png::BitDepth::Eight,
        &[40, 128],
    );
    let palette = png_with(
        2,
        1,
        png::ColorType::Indexed,
        png::BitDepth::Eight,
        &[0, 1],
        |encoder| {
            encoder.set_palette(vec![255, 0, 0, 0, 0, 255]);
            encoder.set_trns(vec![255, 64]);
        },
    );
    let cases = [
        (rgb, [10, 20, 30, 255]),
        (gray, [77, 77, 77, 255]),
        (gray_alpha, [40, 40, 40, 128]),
    ];
    for (bytes, expected) in cases {
        let (_, source) = decode(&bytes, "x.png").unwrap();
        assert_eq!(source.depth, Depth::U8);
        assert_eq!(pixel(&source, 0, 0), expected);
    }
    let (_, source) = decode(&palette, "p.png").unwrap();
    assert_eq!(pixel(&source, 0, 0), [255, 0, 0, 255]);
    assert_eq!(pixel(&source, 1, 0), [0, 0, 255, 64]);
}

#[test]
fn images_larger_than_a_tile_are_split_into_tiles() {
    let (width, height) = (300, 260);
    let data: Vec<u8> = (0..width * height)
        .flat_map(|i| [(i % 251) as u8, (i % 7) as u8, 9, 255])
        .collect();
    let (_, source) = decode(
        &png(
            width,
            height,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &data,
        ),
        "big.png",
    )
    .unwrap();
    assert_eq!(source.tiles.len(), 4);
    let i = 299 + 259 * width;
    assert_eq!(
        pixel(&source, 299, 259),
        [(i % 251) as u16, (i % 7) as u16, 9, 255]
    );
}

fn jpeg(width: u16, height: u16, rgb: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    let encoder = jpeg_encoder::Encoder::new(&mut bytes, 100);
    encoder
        .encode(rgb, width, height, jpeg_encoder::ColorType::Rgb)
        .unwrap();
    bytes
}

#[test]
fn jpeg_decodes_to_opaque_8_bit_rgba() {
    let (w, h) = (16u16, 16u16);
    let rgb: Vec<u8> = (0..u32::from(w) * u32::from(h))
        .flat_map(|_| [200, 120, 40])
        .collect();
    let (_, source) = decode(&jpeg(w, h, &rgb), "b.jpg").unwrap();
    assert_eq!(
        (source.width, source.height, source.depth),
        (16, 16, Depth::U8)
    );
    let p = pixel(&source, 7, 7);
    for (c, expected) in [200u16, 120, 40].into_iter().enumerate() {
        assert!(p[c].abs_diff(expected) <= 3, "{p:?}");
    }
    assert_eq!(p[3], 255);
}

#[test]
fn the_identifier_is_the_hash_of_the_decoded_content() {
    let data = [10u8, 20, 30];
    let (a, _) = decode(
        &png(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &data),
        "a.png",
    )
    .unwrap();
    // The same pixels in another encoding of the same file format.
    let (b, _) = decode(
        &png_with(
            1,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &data,
            |encoder| {
                encoder.set_compression(png::Compression::Fast);
            },
        ),
        "renamed.png",
    )
    .unwrap();
    let (c, _) = decode(
        &png(
            1,
            1,
            png::ColorType::Rgb,
            png::BitDepth::Eight,
            &[10, 20, 31],
        ),
        "c.png",
    )
    .unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert_eq!(a.0.len(), 64);
    assert!(a.0.chars().all(|ch| ch.is_ascii_hexdigit()));
}

/// CRC-32 of PNG chunks, to forge headers.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 != 0 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[test]
fn invalid_and_oversized_files_are_refused_without_crashing() {
    let valid = png(4, 4, png::ColorType::Rgb, png::BitDepth::Eight, &[7; 48]);
    let jpeg = jpeg(16, 16, &[90; 16 * 16 * 3]);
    // A PNG header announcing 70000 × 10 pixels.
    let mut huge = valid.clone();
    huge[16..20].copy_from_slice(&70_000u32.to_be_bytes());
    let crc = crc32(&huge[12..29]);
    huge[29..33].copy_from_slice(&crc.to_be_bytes());
    // 20000 × 20000: 400 Mpx, above the 256 Mpx bound.
    let mut many = valid.clone();
    many[16..20].copy_from_slice(&20_000u32.to_be_bytes());
    many[20..24].copy_from_slice(&20_000u32.to_be_bytes());
    let crc = crc32(&many[12..29]);
    many[29..33].copy_from_slice(&crc.to_be_bytes());

    let mut cases: Vec<(String, Vec<u8>)> = vec![
        ("empty".to_owned(), Vec::new()),
        ("text".to_owned(), b"not an image at all".to_vec()),
        ("huge png".to_owned(), huge),
        ("400 Mpx png".to_owned(), many),
    ];
    for cut in [8, 20, 40, valid.len() - 5] {
        cases.push((format!("png cut at {cut}"), valid[..cut].to_vec()));
    }
    for cut in [2, 30, jpeg.len() / 2] {
        cases.push((format!("jpeg cut at {cut}"), jpeg[..cut].to_vec()));
    }
    let mut corrupted = valid.clone();
    let middle = corrupted.len() / 2;
    corrupted[middle] ^= 0xff;
    cases.push(("png with a flipped byte".to_owned(), corrupted));
    for (what, bytes) in cases {
        let error = decode(&bytes, "bad").expect_err(&what);
        assert!(!error.to_string().is_empty(), "{what}");
    }
    let error = decode(&[0x89, b'P'], "x").unwrap_err();
    assert!(matches!(error, IoError::Format(_)), "{error}");
}

#[test]
fn the_file_importer_resolves_relative_paths_and_reports_missing_files() {
    let dir = std::env::temp_dir().join(format!("cygnus-io-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bytes = png(1, 1, png::ColorType::Rgb, png::BitDepth::Eight, &[1, 2, 3]);
    std::fs::write(dir.join("image.png"), bytes).unwrap();
    let importer = FileImporter::new(&dir);
    let (_, source) = importer.import("image.png").unwrap();
    assert_eq!(source.name, "image.png");
    let absolute = dir.join("image.png");
    importer.import(&absolute.to_string_lossy()).unwrap();
    let error = importer.import("nowhere.png").unwrap_err();
    assert!(error.contains("nowhere.png"), "{error}");
    std::fs::remove_dir_all(&dir).unwrap();
}

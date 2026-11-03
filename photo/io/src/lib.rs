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

//! Image decoding for Cygnus Photo: the PNG and JPEG files of
//! `source.import` (docs/decisions/0020 and 0027).
//!
//! A file is decoded into a [`Source`]: RGBA tiles at its own depth (8 or
//! 16 bits), never modified afterwards, identified by the SHA-256 of its
//! decoded content. ICC profiles are ignored until J6 (sRGB assumed).
//! Invalid files and decompression bombs are refused with an error, never
//! a crash.

use std::path::{Path, PathBuf};

use cygnus_photo_core::pixels::{PixelStore, TileFormat};
use cygnus_photo_core::resolve::Import;
use cygnus_photo_core::{Depth, NodeId, Rect, Source, SourceId};
use sha2::{Digest, Sha256};

/// Largest side of an imported image, in pixels.
pub const MAX_SIDE: u32 = 65_535;
/// Largest number of pixels of an imported image (256 Mpx).
pub const MAX_PIXELS: u64 = 256 * 1024 * 1024;

/// Why a file cannot be imported.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum IoError {
    /// Not a PNG or JPEG file, or a damaged one.
    #[error("{0}")]
    Format(String),
    /// Larger than the bounds of the import.
    #[error(
        "the image is {width} × {height} pixels, larger than the import allows ({MAX_SIDE} pixels per side, 256 Mpx)"
    )]
    TooLarge {
        /// Width announced by the file.
        width: u32,
        /// Height announced by the file.
        height: u32,
    },
    /// The file cannot be read.
    #[error("cannot read {path}: {reason}")]
    Read {
        /// The path, as given.
        path: String,
        /// Why.
        reason: String,
    },
}

fn check_size(width: u32, height: u32) -> Result<(), IoError> {
    if width == 0 || height == 0 {
        return Err(IoError::Format("the image is empty".to_owned()));
    }
    if width > MAX_SIDE || height > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(IoError::TooLarge { width, height });
    }
    Ok(())
}

/// Decoded pixels, straight RGBA, row by row.
struct Pixels {
    width: u32,
    height: u32,
    depth: Depth,
    /// Four values per pixel, 0..=255 or 0..=65535.
    samples: Vec<u16>,
}

/// Decodes a PNG or JPEG file. Returns the identifier of the source (hex
/// SHA-256 of the decoded content) and the source, named `name`.
pub fn decode(bytes: &[u8], name: &str) -> Result<(SourceId, Source), IoError> {
    let pixels = if bytes.starts_with(&[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]) {
        decode_png(bytes)?
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        decode_jpeg(bytes)?
    } else {
        return Err(IoError::Format("not a PNG or JPEG file".to_owned()));
    };
    Ok(into_source(pixels, name))
}

fn decode_png(bytes: &[u8]) -> Result<Pixels, IoError> {
    let format = |error: png::DecodingError| IoError::Format(format!("invalid PNG file: {error}"));
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    // Palette, transparency chunk and low bit depths expanded; 16 bits kept.
    decoder.set_transformations(png::Transformations::EXPAND);
    let header = decoder.read_header_info().map_err(format)?;
    check_size(header.width, header.height)?;
    let mut reader = decoder.read_info().map_err(format)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| IoError::Format("invalid PNG file: image too large".to_owned()))?;
    let mut buffer = vec![0; size];
    let frame = reader.next_frame(&mut buffer).map_err(format)?;
    let (width, height) = (frame.width, frame.height);
    check_size(width, height)?;
    let sixteen = frame.bit_depth == png::BitDepth::Sixteen;
    let values: Vec<u16> = if sixteen {
        buffer[..frame.buffer_size()]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect()
    } else {
        buffer[..frame.buffer_size()]
            .iter()
            .map(|v| u16::from(*v))
            .collect()
    };
    let opaque = if sixteen { u16::MAX } else { 255 };
    let rgba = |pixel: &[u16]| -> [u16; 4] {
        match pixel.len() {
            1 => [pixel[0], pixel[0], pixel[0], opaque],
            2 => [pixel[0], pixel[0], pixel[0], pixel[1]],
            3 => [pixel[0], pixel[1], pixel[2], opaque],
            _ => [pixel[0], pixel[1], pixel[2], pixel[3]],
        }
    };
    let channels = match frame.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(IoError::Format(
                "invalid PNG file: palette not expanded".to_owned(),
            ));
        }
    };
    let row = frame.line_size / if sixteen { 2 } else { 1 };
    let mut samples = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height as usize {
        let line = &values[y * row..y * row + width as usize * channels];
        for pixel in line.chunks_exact(channels) {
            samples.extend(rgba(pixel));
        }
    }
    Ok(Pixels {
        width,
        height,
        depth: if sixteen { Depth::U16 } else { Depth::U8 },
        samples,
    })
}

fn decode_jpeg(bytes: &[u8]) -> Result<Pixels, IoError> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;

    let format = |error: zune_jpeg::errors::DecodeErrors| {
        IoError::Format(format!("invalid JPEG file: {error}"))
    };
    // The scalar path gives the same pixels on every platform (golden
    // images compared across adapters and processors).
    let options = DecoderOptions::default()
        .jpeg_set_out_colorspace(ColorSpace::RGB)
        .set_use_unsafe(false)
        .set_max_width(MAX_SIDE as usize)
        .set_max_height(MAX_SIDE as usize);
    let mut decoder = zune_jpeg::JpegDecoder::new_with_options(ZCursor::new(bytes), options);
    decoder.decode_headers().map_err(format)?;
    let (width, height) = decoder
        .dimensions()
        .ok_or_else(|| IoError::Format("invalid JPEG file: no size".to_owned()))?;
    let (width, height) = (width as u32, height as u32);
    check_size(width, height)?;
    let rgb = decoder.decode().map_err(format)?;
    let expected = width as usize * height as usize * 3;
    if rgb.len() < expected {
        return Err(IoError::Format("invalid JPEG file: truncated".to_owned()));
    }
    let samples = rgb[..expected]
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|p| [u16::from(p[0]), u16::from(p[1]), u16::from(p[2]), 255])
        .collect();
    Ok(Pixels {
        width,
        height,
        depth: Depth::U8,
        samples,
    })
}

fn into_source(pixels: Pixels, name: &str) -> (SourceId, Source) {
    let mut hash = Sha256::new();
    hash.update(pixels.width.to_le_bytes());
    hash.update(pixels.height.to_le_bytes());
    hash.update([match pixels.depth {
        Depth::U8 => 8u8,
        Depth::U16 => 16,
    }]);
    for value in &pixels.samples {
        hash.update(value.to_le_bytes());
    }
    let id = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();

    // Tiles are made by the pixel store of the model, in a scratch layer.
    let mut store = PixelStore::default();
    let scratch = NodeId(0);
    let width = pixels.width as usize;
    store.paint(
        scratch,
        Rect {
            x: 0,
            y: 0,
            width: pixels.width,
            height: pixels.height,
        },
        TileFormat::rgba(pixels.depth),
        |x, y| {
            let i = (y as usize * width + x as usize) * 4;
            [
                pixels.samples[i],
                pixels.samples[i + 1],
                pixels.samples[i + 2],
                pixels.samples[i + 3],
            ]
        },
    );
    let source = Source {
        name: name.to_owned(),
        width: pixels.width,
        height: pixels.height,
        depth: pixels.depth,
        tiles: store.take_layer(scratch),
    };
    (SourceId(id), source)
}

/// Reads and decodes files for `source.import`, relative paths being taken
/// from a base folder (the script folder on the command line, the working
/// folder otherwise).
pub struct FileImporter {
    base: PathBuf,
}

impl FileImporter {
    /// An importer resolving relative paths from `base`.
    pub fn new(base: impl AsRef<Path>) -> Self {
        Self {
            base: base.as_ref().to_path_buf(),
        }
    }

    /// Reads and decodes `path`.
    pub fn read(&self, path: &str) -> Result<(SourceId, Source), IoError> {
        let full = self.base.join(path);
        let bytes = std::fs::read(&full).map_err(|error| IoError::Read {
            path: path.to_owned(),
            reason: error.to_string(),
        })?;
        let name = Path::new(path)
            .file_name()
            .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned());
        decode(&bytes, &name)
    }
}

impl Import for FileImporter {
    fn import(&self, path: &str) -> Result<(SourceId, Source), String> {
        self.read(path).map_err(|error| error.to_string())
    }
}

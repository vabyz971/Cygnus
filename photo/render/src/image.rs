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

//! Exported images: straight-alpha sRGB RGBA, 8 or 16 bits per channel.

use std::io::Write;

/// Bits per channel of an export.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportBits {
    /// 8 bits per channel.
    Eight,
    /// 16 bits per channel.
    Sixteen,
}

impl ExportBits {
    /// Number of bits.
    pub fn bits(self) -> u32 {
        match self {
            Self::Eight => 8,
            Self::Sixteen => 16,
        }
    }

    /// Largest channel value.
    pub fn max_value(self) -> u16 {
        match self {
            Self::Eight => u8::MAX as u16,
            Self::Sixteen => u16::MAX,
        }
    }
}

/// An exported image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Bits per channel.
    pub bits: ExportBits,
    /// RGBA values, row by row, straight alpha, sRGB-encoded.
    pub data: Vec<u16>,
}

/// Why an image could not be read as PNG.
#[derive(Debug, thiserror::Error)]
pub enum PngReadError {
    /// The PNG decoder failed.
    #[error("{0}")]
    Decode(#[from] png::DecodingError),
    /// The PNG is not an RGBA image of 8 or 16 bits.
    #[error("only 8- or 16-bit RGBA PNG images are supported")]
    Format,
}

impl Image {
    /// RGBA values of the pixel at (`x`, `y`).
    pub fn pixel(&self, x: u32, y: u32) -> [u16; 4] {
        let i = ((y * self.width + x) * 4) as usize;
        [
            self.data[i],
            self.data[i + 1],
            self.data[i + 2],
            self.data[i + 3],
        ]
    }

    /// Writes the image as an sRGB PNG.
    pub fn write_png(&self, writer: impl Write) -> Result<(), png::EncodingError> {
        let mut encoder = png::Encoder::new(writer, self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
        let bytes: Vec<u8> = match self.bits {
            ExportBits::Eight => {
                encoder.set_depth(png::BitDepth::Eight);
                self.data
                    .iter()
                    .map(|value| u8::try_from(*value).unwrap_or(u8::MAX))
                    .collect()
            }
            ExportBits::Sixteen => {
                encoder.set_depth(png::BitDepth::Sixteen);
                self.data
                    .iter()
                    .flat_map(|value| value.to_be_bytes())
                    .collect()
            }
        };
        let mut writer = encoder.write_header()?;
        writer.write_image_data(&bytes)
    }

    /// Reads an 8- or 16-bit RGBA PNG from its bytes.
    pub fn read_png(bytes: &[u8]) -> Result<Self, PngReadError> {
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info()?;
        let mut buffer = vec![0; reader.output_buffer_size().ok_or(PngReadError::Format)?];
        let info = reader.next_frame(&mut buffer)?;
        if info.color_type != png::ColorType::Rgba {
            return Err(PngReadError::Format);
        }
        let bytes = &buffer[..info.buffer_size()];
        let (bits, data) = match info.bit_depth {
            png::BitDepth::Eight => (
                ExportBits::Eight,
                bytes.iter().map(|value| u16::from(*value)).collect(),
            ),
            png::BitDepth::Sixteen => (
                ExportBits::Sixteen,
                bytes
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
                    .collect(),
            ),
            _ => return Err(PngReadError::Format),
        };
        Ok(Self {
            width: info.width,
            height: info.height,
            bits,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_round_trip_in_both_depths() {
        for (bits, max) in [(ExportBits::Eight, 255), (ExportBits::Sixteen, 65535)] {
            let image = Image {
                width: 2,
                height: 1,
                bits,
                data: vec![0, 1, max, max, max, 7, 3, 0],
            };
            let mut bytes = Vec::new();
            image.write_png(&mut bytes).unwrap();
            assert_eq!(Image::read_png(&bytes).unwrap(), image);
        }
    }
}

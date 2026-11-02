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

//! The compute format: the pixel format of every intermediate result of the
//! renderer (decoded layer tiles, composited tiles, filter outputs).
//!
//! It is defined here and nowhere else (docs/decisions/0011 and 0026):
//! sRGB-encoded values, premultiplied alpha, 16-bit unsigned integers packed
//! and unpacked by the shaders themselves. Rounding is therefore written in
//! WGSL ([`COMPUTE_CODEC_WGSL`]) instead of being left to the driver's
//! float conversion, which was measured to truncate (0026). Changing the
//! format must only require editing this file; a test enforces that no
//! other source file names it.

/// Texture format of intermediate results.
pub const COMPUTE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Uint;

/// Name of [`COMPUTE_FORMAT`] in WGSL, substituted into shaders when they
/// are assembled.
pub const COMPUTE_FORMAT_WGSL: &str = "rgba16uint";

/// Sampled type of [`COMPUTE_FORMAT`] in WGSL (`texture_2d_array<...>`).
pub const COMPUTE_SAMPLED_WGSL: &str = "u32";

/// WGSL functions converting between f32 colors (0..1) and texels of
/// [`COMPUTE_FORMAT`]. Every shader reading or writing the format uses
/// them: `pack_compute` rounds to the nearest of 65 536 levels (ties to
/// even, `round` in WGSL), `unpack_compute` is exact.
pub const COMPUTE_CODEC_WGSL: &str = "
fn pack_compute(color: vec4<f32>) -> vec4<u32> {
    return vec4<u32>(round(clamp(color, vec4<f32>(0.0), vec4<f32>(1.0)) * 65535.0));
}

fn unpack_compute(texel: vec4<u32>) -> vec4<f32> {
    return vec4<f32>(texel) / 65535.0;
}
";

/// Texture format of the source pool: layer tiles as stored in the
/// document (sRGB-encoded, straight alpha), 8-bit values multiplied by 257
/// (docs/decisions/0021). It happens to be the compute format too.
pub const SOURCE_FORMAT: wgpu::TextureFormat = COMPUTE_FORMAT;

/// Bytes of one texel of [`SOURCE_FORMAT`].
pub const SOURCE_TEXEL_BYTES: u32 = 8;

/// Texture format of the mask pool: gray mask tiles, 8-bit values
/// multiplied by 257.
pub const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R16Uint;

/// Bytes of one texel of [`MASK_FORMAT`].
pub const MASK_TEXEL_BYTES: u32 = 2;

/// WGSL functions reading the source and mask pools: straight 0..1 color,
/// and mask coverage 0..1.
pub const SOURCE_CODEC_WGSL: &str = "
fn unpack_source(texel: vec4<u32>) -> vec4<f32> {
    return vec4<f32>(texel) / 65535.0;
}

fn unpack_mask(texel: vec4<u32>) -> f32 {
    return f32(texel.r) / 65535.0;
}
";

/// One texel of [`COMPUTE_FORMAT`] on the CPU side (uploads, readbacks).
pub type ComputeTexel = [u16; 4];

/// Size in bytes of one [`ComputeTexel`].
pub const COMPUTE_TEXEL_BYTES: u32 = size_of::<ComputeTexel>() as u32;

/// Usages the adapter must allow for [`COMPUTE_FORMAT`]: written by compute
/// shaders, read by every pass, copied for uploads and readbacks.
pub const COMPUTE_FORMAT_USAGES: wgpu::TextureUsages = wgpu::TextureUsages::STORAGE_BINDING
    .union(wgpu::TextureUsages::TEXTURE_BINDING)
    .union(wgpu::TextureUsages::COPY_SRC)
    .union(wgpu::TextureUsages::COPY_DST);

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn texel_size_matches_the_texture_format() {
        let block = COMPUTE_FORMAT.block_copy_size(None).unwrap();
        assert_eq!(block, COMPUTE_TEXEL_BYTES);
    }

    #[test]
    fn pool_texel_sizes_match_their_formats() {
        assert_eq!(
            SOURCE_FORMAT.block_copy_size(None).unwrap(),
            SOURCE_TEXEL_BYTES
        );
        assert_eq!(MASK_FORMAT.block_copy_size(None).unwrap(), MASK_TEXEL_BYTES);
    }

    #[test]
    fn wgsl_name_matches_the_texture_format() {
        // wgpu's Debug name is the WGSL name with different casing.
        let debug = format!("{COMPUTE_FORMAT:?}").to_lowercase();
        assert_eq!(debug, COMPUTE_FORMAT_WGSL);
    }

    fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                sources(&path, found);
            } else if matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("rs" | "wgsl")
            ) {
                found.push(path);
            }
        }
    }

    #[test]
    fn compute_format_is_named_only_here() {
        // Built at run time so that this file does not match itself.
        let names = [
            ["Rgba16", "Uint"].concat(),
            ["rgba16", "uint"].concat(),
            ["R16", "Uint"].concat(),
            ["r16", "uint"].concat(),
        ];
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        for dir in ["src", "tests", "examples"] {
            let dir = root.join(dir);
            if dir.is_dir() {
                sources(&dir, &mut files);
            }
        }
        let this = root.join("src").join("format.rs");
        let offenders: Vec<_> = files
            .iter()
            .filter(|path| **path != this)
            .filter(|path| {
                let text = std::fs::read_to_string(path).unwrap();
                names.iter().any(|name| text.contains(name.as_str()))
            })
            .collect();
        assert!(
            offenders.is_empty(),
            "pixel formats named outside format.rs: {offenders:?}"
        );
    }
}

// Cygnus — Suite créative professionnelle open source
// Copyright (C) 2026 vabyz971
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Image export to exchange formats (PNG / JPEG / WebP / TIFF).
//!
//! - PNG: transparency preserved (infinite canvas included).
//! - JPEG: no alpha support → automatic flattening on a WHITE background
//!   (pro-editor behavior), adjustable quality.
//! - WebP: lossless (the bundled encoder has no lossy mode), alpha kept.
//! - TIFF: uncompressed RGBA, alpha kept.
//!
//! Encoding happens in memory then lands on disk in one write, so the
//! same path serves [`estimate_export_size`].

use std::io::Cursor;
use std::path::Path;

use image::{DynamicImage, ImageBuffer, Rgba};

/// Default JPEG quality (libturbo-jpeg scale 1..=100).
pub const DEFAULT_JPEG_QUALITY: u8 = 90;

/// Pixel budget of the size-estimate sample: beyond it the image is
/// downscaled before encoding and the byte count scaled back up.
pub const ESTIMATE_MAX_PIXELS: u64 = 1_000_000;

/// Requested export format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    /// Transparency preserved
    Png,
    /// Alpha flattened on white; `quality` in 1..=100
    Jpeg { quality: u8 },
    /// Lossless WebP, transparency preserved
    WebP,
    /// Uncompressed TIFF, transparency preserved
    Tiff,
}

impl ExportFormat {
    /// Deduces the format from the path extension (`.png`, `.jpg`,
    /// `.jpeg`, `.webp`, `.tif`, `.tiff`), case-insensitive. Any other
    /// extension → PNG (safe value: lossless, transparency kept).
    #[must_use]
    pub fn from_path(path: &Path) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase);
        match ext.as_deref() {
            Some("jpg") | Some("jpeg") => Self::Jpeg {
                quality: DEFAULT_JPEG_QUALITY,
            },
            Some("webp") => Self::WebP,
            Some("tif") | Some("tiff") => Self::Tiff,
            _ => Self::Png,
        }
    }

    /// Same as [`Self::from_path`] with an explicit JPEG quality.
    #[must_use]
    pub fn from_path_with_quality(path: &Path, quality: u8) -> Self {
        match Self::from_path(path) {
            Self::Jpeg { .. } => Self::Jpeg {
                quality: quality.clamp(1, 100),
            },
            other => other,
        }
    }
}

/// JPEG ignores alpha: the image is composited over an opaque WHITE
/// background (not a mere stripping of the alpha channel, which would
/// render transparent zones black or arbitrary).
#[must_use]
pub fn flatten_on_white(img: &DynamicImage) -> DynamicImage {
    let rgba = img.to_rgba8();
    if rgba.pixels().all(|p| p.0[3] == 255) {
        return DynamicImage::ImageRgba8(rgba); // already opaque: no useful copy
    }
    let (w, h) = rgba.dimensions();
    let mut canvas: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(w.max(1), h.max(1), Rgba([255, 255, 255, 255]));
    image::imageops::overlay(&mut canvas, &rgba, 0, 0);
    DynamicImage::ImageRgba8(canvas)
}

/// Encodes the image in memory.
///
/// # Errors
/// Encoder failure — technical message (English).
pub fn encode_image(img: &DynamicImage, format: ExportFormat) -> Result<Vec<u8>, String> {
    let mut out = Cursor::new(Vec::new());
    match format {
        ExportFormat::Png => img
            .write_to(&mut out, image::ImageFormat::Png)
            .map_err(|e| format!("PNG encoding: {e}"))?,
        ExportFormat::Jpeg { quality } => {
            let flattened = flatten_on_white(img);
            let encoder =
                image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100));
            flattened
                .write_with_encoder(encoder)
                .map_err(|e| format!("JPEG encoding: {e}"))?;
        }
        ExportFormat::WebP => {
            let rgba = DynamicImage::ImageRgba8(img.to_rgba8());
            let encoder = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
            rgba.write_with_encoder(encoder)
                .map_err(|e| format!("WebP encoding: {e}"))?;
        }
        ExportFormat::Tiff => {
            let rgba = DynamicImage::ImageRgba8(img.to_rgba8());
            let encoder = image::codecs::tiff::TiffEncoder::new(&mut out);
            rgba.write_with_encoder(encoder)
                .map_err(|e| format!("TIFF encoding: {e}"))?;
        }
    }
    Ok(out.into_inner())
}

/// Writes the image to disk in the requested format.
///
/// # Errors
/// Encoding or disk-write error — technical message (English).
pub fn export_image(img: &DynamicImage, path: &Path, format: ExportFormat) -> Result<(), String> {
    let bytes = encode_image(img, format)?;
    std::fs::write(path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))
}

/// Estimated file size in bytes for `format`.
///
/// Images above [`ESTIMATE_MAX_PIXELS`] are downscaled first and the
/// encoded size scaled by the area ratio: an order of magnitude for the
/// export dialog, not an exact count (compression ratios shift a little
/// with resolution).
///
/// # Errors
/// Encoder failure (same messages as [`encode_image`]).
pub fn estimate_export_size(img: &DynamicImage, format: ExportFormat) -> Result<u64, String> {
    let (w, h) = (u64::from(img.width()), u64::from(img.height()));
    let pixels = w * h;
    if pixels <= ESTIMATE_MAX_PIXELS {
        return encode_image(img, format).map(|b| b.len() as u64);
    }
    let scale = (ESTIMATE_MAX_PIXELS as f64 / pixels as f64).sqrt();
    let sw = ((w as f64 * scale).round() as u32).max(1);
    let sh = ((h as f64 * scale).round() as u32).max(1);
    let sample = img.resize_exact(sw, sh, image::imageops::FilterType::Triangle);
    let bytes = encode_image(&sample, format)?.len() as f64;
    let ratio = pixels as f64 / (f64::from(sw) * f64::from(sh));
    Ok((bytes * ratio).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_transparent() -> DynamicImage {
        // Opaque red left half, TRANSPARENT right half
        let mut b = ImageBuffer::from_pixel(4, 2, Rgba([0, 0, 0, 0]));
        for y in 0..2 {
            for x in 0..2 {
                b.put_pixel(x, y, Rgba([255, 0, 0, 255]));
            }
        }
        DynamicImage::ImageRgba8(b)
    }

    fn temp_path(tag: &str, ext: &str) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| std::time::Duration::from_secs(0))
            .as_nanos();
        std::env::temp_dir().join(format!("cyg-export-{tag}-{}.{}", nanos, ext))
    }

    #[test]
    fn format_deduit_de_lextension_insensible_casse() {
        assert_eq!(
            ExportFormat::from_path(Path::new("a.png")),
            ExportFormat::Png
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("a.PNG")),
            ExportFormat::Png
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("b.JpG")),
            ExportFormat::Jpeg {
                quality: DEFAULT_JPEG_QUALITY
            }
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("c.jpeg")),
            ExportFormat::Jpeg {
                quality: DEFAULT_JPEG_QUALITY
            }
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("d.WebP")),
            ExportFormat::WebP
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("e.tif")),
            ExportFormat::Tiff
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("e.TIFF")),
            ExportFormat::Tiff
        );
        // Unknown extension: default PNG (safe, lossless)
        assert_eq!(
            ExportFormat::from_path(Path::new("d.bmp")),
            ExportFormat::Png
        );
        assert_eq!(
            ExportFormat::from_path(Path::new("sans-ext")),
            ExportFormat::Png
        );
    }

    #[test]
    fn png_conserve_la_transparence() {
        let img = sample_transparent();
        let path = temp_path("png", "png");
        export_image(&img, &path, ExportFormat::Png).expect("export png");

        let reloaded = image::open(&path).expect("relecture");
        std::fs::remove_file(&path).ok();
        let rgba = reloaded.to_rgba8();
        let p = rgba.get_pixel(3, 0);
        assert_eq!(p[3], 0, "alpha conservé en PNG");
    }

    #[test]
    fn jpeg_aplatit_sur_blanc_et_perd_l_alpha() {
        let img = sample_transparent();
        let path = temp_path("jpg", "jpg");
        export_image(&img, &path, ExportFormat::Jpeg { quality: 95 }).expect("export jpg");

        let reloaded = image::open(&path).expect("relecture");
        std::fs::remove_file(&path).ok();

        // Transparent zone → pure white (compression tolerance)
        let rgb = reloaded.to_rgb8();
        let bg = rgb.get_pixel(3, 0);
        assert!(
            i16::from(bg[0]) >= 252 && i16::from(bg[1]) >= 252 && i16::from(bg[2]) >= 252,
            "fond attendu blanc, obtenu {bg:?}"
        );
        // Opaque red zone → stays red-dominated
        let fg = rgb.get_pixel(0, 0);
        assert!(
            fg[0] > 200 && fg[1] < 80 && fg[2] < 80,
            "rouge attendu, obtenu {fg:?}"
        );
    }

    #[test]
    fn image_deja_opaque_n_est_pas_recopiee_par_flatten() {
        let opaque =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(2, 2, Rgba([10, 20, 30, 255])));
        // Same pixel content: the fast path skips the useless overlay
        let out = flatten_on_white(&opaque);
        assert_eq!(*out.to_rgba8(), *opaque.to_rgba8());
    }

    #[test]
    fn quality_only_applies_to_jpeg() {
        assert_eq!(
            ExportFormat::from_path_with_quality(Path::new("a.jpg"), 0),
            ExportFormat::Jpeg { quality: 1 }
        );
        assert_eq!(
            ExportFormat::from_path_with_quality(Path::new("a.webp"), 50),
            ExportFormat::WebP
        );
    }

    #[test]
    fn webp_and_tiff_keep_pixels_and_alpha() {
        let img = sample_transparent();
        for (format, ext) in [(ExportFormat::WebP, "webp"), (ExportFormat::Tiff, "tiff")] {
            let path = temp_path(ext, ext);
            export_image(&img, &path, format).expect("export");
            let reloaded = image::open(&path).expect("relecture");
            std::fs::remove_file(&path).ok();
            // Lossless: exact round-trip, transparency included
            assert_eq!(*reloaded.to_rgba8(), *img.to_rgba8(), "{ext}");
        }
    }

    #[test]
    fn estimate_matches_small_encodes_exactly() {
        let img = sample_transparent();
        for format in [
            ExportFormat::Png,
            ExportFormat::Jpeg { quality: 80 },
            ExportFormat::WebP,
            ExportFormat::Tiff,
        ] {
            let exact = encode_image(&img, format).expect("encode").len() as u64;
            assert_eq!(estimate_export_size(&img, format), Ok(exact), "{format:?}");
        }
    }

    #[test]
    fn estimate_of_large_tiff_is_close() {
        // Uncompressed: size grows linearly with area, so the scaled
        // estimate must land within a few percent of the real file.
        let img =
            DynamicImage::ImageRgba8(ImageBuffer::from_pixel(1500, 1000, Rgba([1, 2, 3, 255])));
        let exact = encode_image(&img, ExportFormat::Tiff)
            .expect("encode")
            .len() as f64;
        let estimate = estimate_export_size(&img, ExportFormat::Tiff).expect("estimate") as f64;
        assert!(
            (estimate - exact).abs() / exact < 0.05,
            "{estimate} vs {exact}"
        );
    }

    #[test]
    fn jpeg_quality_shrinks_estimate() {
        let mut b = ImageBuffer::new(64, 64);
        for (x, y, p) in b.enumerate_pixels_mut() {
            *p = Rgba([(x * 4) as u8, (y * 4) as u8, ((x ^ y) * 4) as u8, 255]);
        }
        let img = DynamicImage::ImageRgba8(b);
        let high = estimate_export_size(&img, ExportFormat::Jpeg { quality: 95 }).expect("hq");
        let low = estimate_export_size(&img, ExportFormat::Jpeg { quality: 20 }).expect("lq");
        assert!(low < high);
    }
}

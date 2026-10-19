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

//! Text measurement: the trait layout calls and `text-engine`
//! will implement for real.
//!
//! Layout needs EXTENTS (width/height of a span within a
//! max width), never glyphs: shaping (shaping, fragmentation,
//! fonts) lives in `text-engine`. [`NullMeasurer`] provides deterministic
//! advances for tests and fontless mockups.

/// Measures a text span: `(width, height)` for `font_size` within
/// `max_width` (returns `\n` = lines). Pure and deterministic.
pub trait TextMeasurer {
    /// Measures `text` (`font_size` points, `max_width` envelope).
    fn measure(&self, text: &str, font_size: f32, max_width: f32) -> (f32, f32);
}

/// Deterministic null measurer: `0.6 × size` advance per character,
/// `1.2 × size` leading, lines broken at `max_width` (no breaking —
/// fine wrapping belongs to the real shaper).
#[derive(Debug, Clone, Copy, Default)]
pub struct NullMeasurer;

impl TextMeasurer for NullMeasurer {
    fn measure(&self, text: &str, font_size: f32, max_width: f32) -> (f32, f32) {
        let size = font_size.max(0.0);
        let max_width = max_width.max(0.0);
        let mut width: f32 = 0.0;
        let mut lines = 0u32;
        for line in text.split('\n') {
            lines += 1;
            width = width.max((line.chars().count() as f32 * 0.6 * size).min(max_width));
        }
        (width, lines as f32 * 1.2 * size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mesure_nulle_deterministe() {
        let m = NullMeasurer;
        assert_eq!(m.measure("ab", 10.0, 1000.0), (12.0, 12.0));
        assert_eq!(m.measure("a\nb", 10.0, 1000.0), (6.0, 24.0));
        assert_eq!(m.measure("abcdef", 10.0, 20.0), (20.0, 12.0));
        assert_eq!(m.measure("", 10.0, 100.0), (0.0, 12.0));
    }

    /// Layout never shapes: this adapter (test-only)
    /// proves `text-engine` can feed `TextMeasurer` without
    /// layout depending on shaping at runtime (dev-dependency only).
    #[test]
    fn text_engine_nourrit_le_mesureur() {
        struct EngineMeasurer;
        impl TextMeasurer for EngineMeasurer {
            fn measure(&self, text: &str, font_size: f32, max_width: f32) -> (f32, f32) {
                let mut model = text_engine::TextModel::new();
                model.push(text_engine::Span::new(text, "F", font_size));
                text_engine::measure_text(&model, &text_engine::FixedFontProvider, max_width)
            }
        }

        let m = EngineMeasurer;
        // Same result as the direct shaper (one line, 0.6 advances).
        assert_eq!(m.measure("ab", 10.0, 1000.0), (12.0, 12.0));
        // …and breaking passes through the adapter.
        assert_eq!(m.measure("aa bb", 10.0, 20.0), (12.0, 24.0));
    }
}

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

//! Text layout: line breaking + shaped runs (advances).
//!
//! REAL shaping (kerning, ligatures, complex scripts) belongs to a
//! future shaper plugged into [`FontProvider`]; this module does the work
//! HONESTLY doable without fonts: per-span splitting, greedy
//! space-based breaking within `max_width`, cumulative advances. [`GlyphRun`]s
//! carry characters + advances + offsets — the render graph will consume them
//! without going back through layout.

use datatypes::Rect;

use crate::{Span, TextModel};

/// Font metrics: advances and line heights (implemented for real
/// later — HarfBuzz/fontations behind this trait, model unchanged).
pub trait FontProvider {
    /// Advance of a character at the given size.
    fn advance(&self, ch: char, size: f32) -> f32;
    /// Line height at the given size.
    fn line_height(&self, size: f32) -> f32;
}

/// Deterministic fixed font (tests, mockups): `0.6 × size`
/// advance (space: `0.3 × size`), `1.2 × size` leading.
#[derive(Debug, Clone, Copy, Default)]
pub struct FixedFontProvider;

impl FontProvider for FixedFontProvider {
    fn advance(&self, ch: char, size: f32) -> f32 {
        let size = size.max(0.0);
        if ch == ' ' { 0.3 * size } else { 0.6 * size }
    }

    fn line_height(&self, size: f32) -> f32 {
        1.2 * size.max(0.0)
    }
}

/// Shaped glyph: character + advance + offset within its run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShapedGlyph {
    /// Source character.
    pub ch: char,
    /// Advance (drawing units).
    pub advance: f32,
    /// X offset from the start of the run.
    pub x: f32,
}

/// Glyph run: fragment of a span on one line (same font/size).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlyphRun {
    /// Index of the source span in the model.
    pub span: usize,
    /// Family (copied for the backend).
    pub family: String,
    /// Size (copied).
    pub size: f32,
    /// X offset of the run within the line.
    pub x: f32,
    /// Glyphs in order.
    pub glyphs: Vec<ShapedGlyph>,
}

impl GlyphRun {
    /// Width (sum of advances).
    #[must_use]
    pub fn width(&self) -> f32 {
        self.glyphs.iter().map(|g| g.advance).sum()
    }
}

/// Composed line: runs + metrics.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LaidOutLine {
    /// Runs in order.
    pub runs: Vec<GlyphRun>,
    /// Total width.
    pub width: f32,
    /// Height (max of line heights).
    pub height: f32,
}

/// Composed text: lines + bounds.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LaidOutText {
    /// Lines in order.
    pub lines: Vec<LaidOutLine>,
    /// Bounding box (origin 0,0).
    pub bounds: Rect,
}

impl LaidOutText {
    /// Empty.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            lines: Vec::new(),
            bounds: Rect::EMPTY,
        }
    }

    /// Number of lines.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// All runs in order (lines then runs).
    #[must_use]
    pub fn glyph_runs(&self) -> Vec<&GlyphRun> {
        self.lines.iter().flat_map(|l| l.runs.iter()).collect()
    }
}

/// Lays out the model within `max_width` (≤ 0 = one line per `\n`, no
/// breaking). Greedy space-based breaking, spans never merged.
#[must_use]
pub fn layout_text(model: &TextModel, provider: &dyn FontProvider, max_width: f32) -> LaidOutText {
    let wrap_at = if max_width > 0.0 {
        max_width
    } else {
        f32::INFINITY
    };
    let mut lines: Vec<LaidOutLine> = Vec::new();
    let mut current = LaidOutLine {
        runs: Vec::new(),
        width: 0.0,
        height: 0.0,
    };
    let empty_line = || LaidOutLine {
        runs: Vec::new(),
        width: 0.0,
        height: 0.0,
    };
    for (span_index, span) in model.spans.iter().enumerate() {
        // `split` emits N+1 segments for N `\n`s: each segment is shaped,
        // each `\n` ends the line (empty segments = empty lines).
        let parts: Vec<&str> = span.text.split('\n').collect();
        for (i, hard) in parts.iter().enumerate() {
            shape_segment(
                provider,
                &mut current,
                span_index,
                span,
                hard,
                wrap_at,
                &mut lines,
            );
            if i + 1 < parts.len() {
                lines.push(std::mem::replace(&mut current, empty_line()));
            }
        }
    }
    if !current.runs.is_empty() {
        lines.push(current);
    }
    let height: f32 = lines.iter().map(|l| l.height).sum();
    let width: f32 = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    LaidOutText {
        lines,
        bounds: if height > 0.0 {
            Rect::from_xywh(0.0, 0.0, width, height)
        } else {
            Rect::EMPTY
        },
    }
}

/// Shapes a `\n`-free segment: splits at spaces on overflow.
#[allow(clippy::too_many_arguments)]
fn shape_segment(
    provider: &dyn FontProvider,
    current: &mut LaidOutLine,
    span_index: usize,
    span: &Span,
    segment: &str,
    wrap_at: f32,
    lines: &mut Vec<LaidOutLine>,
) {
    // Splits into words keeping spaces (leading spaces of a
    // fresh line are dropped — no stray indentation on wrap).
    let mut words: Vec<&str> = Vec::new();
    let mut start = 0;
    let bytes = segment.as_bytes();
    let mut i = 0;
    while i < segment.len() {
        if bytes[i] == b' ' {
            if i > start {
                words.push(&segment[start..i]);
            }
            words.push(" ");
            start = i + 1;
        }
        i += 1;
    }
    if start < segment.len() {
        words.push(&segment[start..]);
    }
    if words.is_empty() {
        return;
    }
    let mut words = words.into_iter().peekable();
    while let Some(word) = words.next() {
        if word == " " && current.runs.is_empty() {
            continue;
        }
        let word_w: f32 = word.chars().map(|ch| provider.advance(ch, span.size)).sum();
        if current.width > 0.0 && current.width + word_w > wrap_at && word != " " {
            trim_trailing_spaces(current);
            let finished = std::mem::replace(
                current,
                LaidOutLine {
                    runs: Vec::new(),
                    width: 0.0,
                    height: 0.0,
                },
            );
            lines.push(finished);
        }
        push_word(provider, current, span_index, span, word);
        // End-of-line space consumed: the next line starts clean.
        if word == " " && current.width >= wrap_at && words.peek().is_some() {
            let finished = std::mem::replace(
                current,
                LaidOutLine {
                    runs: Vec::new(),
                    width: 0.0,
                    height: 0.0,
                },
            );
            lines.push(finished);
        }
    }
}

/// Trims end-of-line spaces (a line's width ignores
/// its trailing space — the next word starts on a clean line).
fn trim_trailing_spaces(current: &mut LaidOutLine) {
    while let Some(run) = current.runs.last_mut() {
        let trailing = run.glyphs.iter().rev().take_while(|g| g.ch == ' ').count();
        if trailing == 0 {
            break;
        }
        for _ in 0..trailing {
            if let Some(glyph) = run.glyphs.pop() {
                current.width -= glyph.advance;
            }
        }
        if run.glyphs.is_empty() {
            current.runs.pop();
        }
    }
    current.width = current.width.max(0.0);
}

/// Appends a word to the current run (merged if same span).
fn push_word(
    provider: &dyn FontProvider,
    current: &mut LaidOutLine,
    span_index: usize,
    span: &Span,
    word: &str,
) {
    let height = provider.line_height(span.size);
    current.height = current.height.max(height);
    let reuse = current
        .runs
        .last()
        .is_some_and(|run| run.span == span_index);
    if !reuse {
        current.runs.push(GlyphRun {
            span: span_index,
            family: span.family.clone(),
            size: span.size,
            x: current.width,
            glyphs: Vec::new(),
        });
    }
    let run = current.runs.last_mut().expect("run pushed");
    for ch in word.chars() {
        let advance = provider.advance(ch, span.size);
        run.glyphs.push(ShapedGlyph {
            ch,
            advance,
            x: current.width - run.x,
        });
        current.width += advance;
    }
}

/// `layout-engine`-compatible measurement: `(width, height)` via this engine
/// (layout never shapes itself — it delegates here).
#[must_use]
pub fn measure_text(model: &TextModel, provider: &dyn FontProvider, max_width: f32) -> (f32, f32) {
    let laid = layout_text(model, provider, max_width);
    (laid.bounds.width(), laid.bounds.height())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn modele(texte: &str) -> TextModel {
        let mut model = TextModel::new();
        model.push(Span::new(texte, "F", 10.0));
        model
    }

    #[test]
    fn ligne_simple_un_run() {
        let laid = layout_text(&modele("ab"), &FixedFontProvider, 1000.0);
        assert_eq!(laid.line_count(), 1);
        assert_eq!(laid.glyph_runs().len(), 1);
        assert!((laid.bounds.width() - 12.0).abs() < 0.001);
        let run = &laid.glyph_runs()[0];
        assert_eq!(run.glyphs.iter().map(|g| g.ch).collect::<String>(), "ab");
    }

    #[test]
    fn césure_aux_espaces() {
        // "aa bb": 2×12 + space 3 = 27; wrap at 20 ⇒ "aa" / "bb".
        let laid = layout_text(&modele("aa bb"), &FixedFontProvider, 20.0);
        assert_eq!(laid.line_count(), 2);
        assert!((laid.bounds.width() - 12.0).abs() < 0.001);
    }

    #[test]
    fn sauts_durs_conserves() {
        let laid = layout_text(&modele("a\n\nb"), &FixedFontProvider, 1000.0);
        assert_eq!(laid.line_count(), 3);
    }

    #[test]
    fn spans_separes_runs_separes() {
        let mut model = TextModel::new();
        model.push(Span::new("a", "F", 10.0));
        model.push(Span::new("b", "G", 12.0));
        let laid = layout_text(&model, &FixedFontProvider, 1000.0);
        assert_eq!(laid.line_count(), 1);
        assert_eq!(laid.glyph_runs().len(), 2);
        assert_eq!(laid.glyph_runs()[1].family, "G");
    }

    #[test]
    fn mesure_pour_layout() {
        assert_eq!(
            measure_text(&modele("ab"), &FixedFontProvider, 1000.0),
            (12.0, 12.0)
        );
    }
}

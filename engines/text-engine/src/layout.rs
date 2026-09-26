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

//! Layout texte : coupure de lignes + runs façonnés (avances).
//!
//! Le façonnage RÉEL (kernings, ligatures, scriptes complexes) appartient à
//! un futur façonneur branché sur [`FontProvider`] ; ce module fait le
//! travail HONNÊTEMENT faisable sans fontes : découpage par span, césure
//! gloutonne aux espaces dans `max_width`, avances cumulées. Les [`GlyphRun`]
//! portent caractères + avances + offsets — le render graph les consommera
//! sans repasser par le layout.

use datatypes::Rect;

use crate::{Span, TextModel};

/// Métriques de fonte : avances et interlignes (implémenté pour de vrai
/// plus tard — HarfBuzz/fontations derrière ce trait, modèle inchangé).
pub trait FontProvider {
    /// Avance d'un caractère au corps donné.
    fn advance(&self, ch: char, size: f32) -> f32;
    /// Hauteur de ligne au corps donné.
    fn line_height(&self, size: f32) -> f32;
}

/// Fonte fixe déterministe (tests, maquettes) : avance `0.6 × corps`
/// (espace : `0.3 × corps`), interligne `1.2 × corps`.
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

/// Glyphe façonné : caractère + avance + offset dans son run.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ShapedGlyph {
    /// Caractère source.
    pub ch: char,
    /// Avance (unités dessin).
    pub advance: f32,
    /// Offset X depuis le début du run.
    pub x: f32,
}

/// Run de glyphes : fragment d'un span sur une ligne (même fonte/taille).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GlyphRun {
    /// Index du span source dans le modèle.
    pub span: usize,
    /// Famille (recopiée pour le backend).
    pub family: String,
    /// Corps (recopié).
    pub size: f32,
    /// Offset X du run dans la ligne.
    pub x: f32,
    /// Glyphes dans l'ordre.
    pub glyphs: Vec<ShapedGlyph>,
}

impl GlyphRun {
    /// Largeur (somme des avances).
    #[must_use]
    pub fn width(&self) -> f32 {
        self.glyphs.iter().map(|g| g.advance).sum()
    }
}

/// Ligne composée : runs + métriques.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LaidOutLine {
    /// Runs dans l'ordre.
    pub runs: Vec<GlyphRun>,
    /// Largeur totale.
    pub width: f32,
    /// Hauteur (max des interlignes).
    pub height: f32,
}

/// Texte composé : lignes + bornes.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LaidOutText {
    /// Lignes dans l'ordre.
    pub lines: Vec<LaidOutLine>,
    /// Englobant (origine 0,0).
    pub bounds: Rect,
}

impl LaidOutText {
    /// Vide.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            lines: Vec::new(),
            bounds: Rect::EMPTY,
        }
    }

    /// Nombre de lignes.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Tous les runs dans l'ordre (lignes puis runs).
    #[must_use]
    pub fn glyph_runs(&self) -> Vec<&GlyphRun> {
        self.lines.iter().flat_map(|l| l.runs.iter()).collect()
    }
}

/// Compose le modèle dans `max_width` (≤ 0 = une ligne par `\n`, sans
/// césure). Coupure gloutonne aux espaces, spans jamais fusionnés.
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
        // `split` émet N+1 segments pour N `\n` : chaque segment est façonné,
        // chaque `\n` termine la ligne (segments vides = lignes vides).
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

/// Façonne un segment sans `\n` : découpe aux espaces si dépassement.
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
    // Découpe en mots en conservant les espaces (les espaces de tête d'une
    // ligne fraîche sont écrasés — pas d'indentation parasite au wrap).
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
        // Espace de fin de ligne consommé : la ligne suivante repart propre.
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

/// Retire les espaces de fin de ligne (la largeur d'une ligne ne compte
/// pas son espace traînant — le mot suivant repart sur une ligne propre).
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

/// Ajoute un mot au run courant (fusionné si même span).
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

/// Mesure compatible `layout-engine` : `(largeur, hauteur)` via ce moteur
/// (le layout ne façonne jamais lui-même — il délègue ici).
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
        // "aa bb" : 2×12 + espace 3 = 27 ; wrap à 20 ⇒ "aa" / "bb".
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

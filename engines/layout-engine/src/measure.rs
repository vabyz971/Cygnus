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

//! Mesure du texte : le trait que le layout appelle et que `text-engine`
//! implémentera pour de vrai.
//!
//! Le layout a besoin d'EXTENSIONS (largeur/hauteur d'un span dans une
//! largeur max), jamais de glyphes : le façonnage (shaping, fragmentation,
//! fontes) vit dans `text-engine`. [`NullMeasurer`] donne des avances
//! déterministes pour les tests et les maquettes sans fontes.

/// Mesure d'un span texte : `(largeur, hauteur)` pour `font_size` dans
/// `max_width` (retours `\n` = lignes). Pure et déterministe.
pub trait TextMeasurer {
    /// Mesure `text` (points `font_size`, enveloppe `max_width`).
    fn measure(&self, text: &str, font_size: f32, max_width: f32) -> (f32, f32);
}

/// Mesureur nul déterministe : avance `0.6 × corps` par caractère,
/// interligne `1.2 × corps`, lignes coupées à `max_width` (sans césure —
/// l'habillage fin appartient au façonneur réel).
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

    /// Le layout ne façonne jamais : cet adaptateur (test uniquement)
    /// prouve que `text-engine` peut nourrir `TextMeasurer` sans que le
    /// layout ne dépende du façonnage au runtime (dev-dependency seule).
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
        // Même résultat que le façonneur direct (une ligne, avances 0.6).
        assert_eq!(m.measure("ab", 10.0, 1000.0), (12.0, 12.0));
        // …et la césure traverse l'adaptateur.
        assert_eq!(m.measure("aa bb", 10.0, 20.0), (12.0, 24.0));
    }
}

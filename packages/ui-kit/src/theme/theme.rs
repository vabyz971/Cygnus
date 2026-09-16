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

//! Thème central Cygnus : assemblage des tokens visuels.
//!
//! [`CygnusTheme`] est la SEULE source des valeurs visuelles
//! (couleurs, espacements, rayons, typo, tailles, bordures). Les
//! composants reçoivent le thème en paramètre (`&CygnusTheme`) et ne
//! codent jamais de valeur en dur. Pour la compatibilité, les types
//! restent aussi accessibles via [`super::tokens`].

use super::borders::CygnusBorders;
use super::colors::CygnusColors;
use super::radius::CygnusRadius;
use super::sizes::CygnusSizes;
use super::spacing::CygnusSpacing;
use super::typography::CygnusTypography;

/// Thème complet de Cygnus.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusTheme {
    /// Couleurs.
    pub colors: CygnusColors,
    /// Espacements.
    pub spacing: CygnusSpacing,
    /// Rayons.
    pub radius: CygnusRadius,
    /// Typographie.
    pub typography: CygnusTypography,
    /// Tailles de contrôles.
    pub sizes: CygnusSizes,
    /// Bordures.
    pub borders: CygnusBorders,
}

impl CygnusTheme {
    /// Thème sombre par défaut de Cygnus.
    pub fn dark() -> Self {
        Self {
            colors: CygnusColors::dark(),
            spacing: CygnusSpacing::dark(),
            radius: CygnusRadius::dark(),
            typography: CygnusTypography::dark(),
            sizes: CygnusSizes::dark(),
            borders: CygnusBorders::dark(),
        }
    }
}

impl Default for CygnusTheme {
    /// Thème sombre par défaut.
    fn default() -> Self {
        Self::dark()
    }
}

/// Luminance relative approximative (canal par canal, sans gamma).
#[cfg(test)]
fn luminance(color: egui::Color32) -> f32 {
    0.2126 * f32::from(color.r()) + 0.7152 * f32::from(color.g()) + 0.0722 * f32::from(color.b())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dark_theme_colors_distinct() {
        let theme = CygnusTheme::dark();
        assert_ne!(theme.colors.bg_primary, theme.colors.bg_secondary);
        assert_ne!(theme.colors.bg_secondary, theme.colors.bg_tertiary);
        assert_ne!(theme.colors.bg_primary, theme.colors.bg_tertiary);
        assert_ne!(theme.colors.fg_primary, theme.colors.fg_secondary);
        assert_ne!(theme.colors.accent, theme.colors.accent_hover);
    }

    #[test]
    fn dark_theme_fg_contrasts_bg() {
        // Le texte principal doit fortement contraster avec le fond principal.
        let theme = CygnusTheme::dark();
        let contrast =
            (luminance(theme.colors.fg_primary) - luminance(theme.colors.bg_primary)).abs();
        assert!(contrast > 150.0, "contraste fg/bg trop faible : {contrast}");
    }

    #[test]
    fn dark_theme_spacing_ordered() {
        let spacing = CygnusTheme::dark().spacing;
        assert!(spacing.xs < spacing.sm);
        assert!(spacing.sm < spacing.md);
        assert!(spacing.md < spacing.lg);
        assert!(spacing.lg < spacing.xl);
    }

    #[test]
    fn dark_theme_sizes_positive() {
        let sizes = CygnusTheme::dark().sizes;
        for size in [
            sizes.button_sm,
            sizes.button_md,
            sizes.button_lg,
            sizes.input_height,
            sizes.icon_button,
            sizes.tab_height,
            sizes.min_touch,
        ] {
            assert!(size > 0.0, "taille non positive : {size}");
        }
        assert!(sizes.button_sm < sizes.button_md);
        assert!(sizes.button_md < sizes.button_lg);
    }
}

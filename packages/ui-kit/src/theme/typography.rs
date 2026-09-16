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

//! Échelle typographique de Cygnus.
//!
//! SEULE source des tailles de texte : aucun widget ne doit coder une
//! taille en dur. Les assistants [`heading_text`], [`body_text`] et
//! [`caption_text`] construisent des [`egui::RichText`] calibrés.

use super::theme::CygnusTheme;

/// Tailles typographiques du thème.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusTypography {
    /// Titres de panneaux.
    pub heading_size: f32,
    /// Texte courant.
    pub body_size: f32,
    /// Légendes et textes secondaires.
    pub caption_size: f32,
    /// Icônes.
    pub icon_size: f32,
}

impl CygnusTypography {
    /// Échelle du thème sombre.
    pub fn dark() -> Self {
        Self {
            heading_size: 16.0,
            body_size: 13.0,
            caption_size: 11.0,
            icon_size: 18.0,
        }
    }
}

impl Default for CygnusTypography {
    /// Échelle par défaut.
    fn default() -> Self {
        Self::dark()
    }
}

/// Taille du texte des titres de panneaux.
pub fn heading_size(theme: &CygnusTheme) -> f32 {
    theme.typography.heading_size
}

/// Taille du texte courant.
pub fn body_size(theme: &CygnusTheme) -> f32 {
    theme.typography.body_size
}

/// Taille des légendes et textes secondaires.
pub fn caption_size(theme: &CygnusTheme) -> f32 {
    theme.typography.caption_size
}

/// Taille des icônes.
pub fn icon_size(theme: &CygnusTheme) -> f32 {
    theme.typography.icon_size
}

/// Texte formaté en style titre.
pub fn heading_text(theme: &CygnusTheme, text: &str) -> egui::RichText {
    egui::RichText::new(text).size(heading_size(theme)).strong()
}

/// Texte formaté en style courant.
pub fn body_text(theme: &CygnusTheme, text: &str) -> egui::RichText {
    egui::RichText::new(text).size(body_size(theme))
}

/// Texte formaté en style légende (couleur secondaire du thème).
pub fn caption_text(theme: &CygnusTheme, text: &str) -> egui::RichText {
    egui::RichText::new(text)
        .size(caption_size(theme))
        .color(theme.colors.fg_secondary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_scale_is_ordered() {
        let theme = CygnusTheme::dark();
        assert!(caption_size(&theme) < body_size(&theme));
        assert!(body_size(&theme) < heading_size(&theme));
    }

    #[test]
    fn helpers_do_not_panic() {
        let theme = CygnusTheme::dark();
        let _ = heading_text(&theme, "Titre");
        let _ = body_text(&theme, "Corps");
        let _ = caption_text(&theme, "Legende");
    }
}

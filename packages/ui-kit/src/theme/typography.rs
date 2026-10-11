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

//! Cygnus type scale.
//!
//! SINGLE source of text sizes: no widget may hardcode a
//! size. The [`heading_text`], [`body_text`] and
//! [`caption_text`] helpers build calibrated [`egui::RichText`].

use super::theme::CygnusTheme;

/// Theme typographic sizes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CygnusTypography {
    /// Panel headings.
    pub heading_size: f32,
    /// Body text.
    pub body_size: f32,
    /// Captions and secondary text.
    pub caption_size: f32,
    /// Icons.
    pub icon_size: f32,
}

impl CygnusTypography {
    /// Dark-theme scale.
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
    /// Default scale.
    fn default() -> Self {
        Self::dark()
    }
}

/// Panel-heading text size.
pub fn heading_size(theme: &CygnusTheme) -> f32 {
    theme.typography.heading_size
}

/// Body text size.
pub fn body_size(theme: &CygnusTheme) -> f32 {
    theme.typography.body_size
}

/// Caption and secondary-text size.
pub fn caption_size(theme: &CygnusTheme) -> f32 {
    theme.typography.caption_size
}

/// Icon size.
pub fn icon_size(theme: &CygnusTheme) -> f32 {
    theme.typography.icon_size
}

/// Text formatted in heading style.
pub fn heading_text(theme: &CygnusTheme, text: &str) -> egui::RichText {
    egui::RichText::new(text).size(heading_size(theme)).strong()
}

/// Text formatted in body style.
pub fn body_text(theme: &CygnusTheme, text: &str) -> egui::RichText {
    egui::RichText::new(text).size(body_size(theme))
}

/// Text formatted in caption style (theme secondary color).
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

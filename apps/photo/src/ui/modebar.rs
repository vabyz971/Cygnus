// Cygnus — Professional open-source creative suite
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

//! PHOTO mode and tool-settings bar (2nd top row,
//! full width, domain widget).
//!
//! Row 1: large mode-switch buttons (Vector, Pixel,
//! Layout) then a separator. Row 2: name and settings of the
//! selected tool (delegated to `super::optionsbar`). The mode restricts
//! the rail tools (paint tools live in Pixel; Vector and
//! Layout expose move, hand, and zoom until their
//! own tools land): switching modes falls back to Move when
//! the current tool does not exist there.

use super::optionsbar::draw_tool_options;
use super::viewport::{PhotoBrushSettings, PhotoCanvasTool};
use cygnus_ui::components::{Button, ButtonVariant};
use cygnus_ui::theme::CygnusTheme;

/// Document edit mode (persona-style).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoEditMode {
    /// Vector paths (move, hand, zoom for now).
    Vector,
    /// Pixel retouch (all tools).
    #[default]
    Pixel,
    /// Page layout (move, hand, zoom for now).
    Layout,
}

impl PhotoEditMode {
    /// Mode label (ASCII).
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Vector => "Vector",
            Self::Pixel => "Pixel",
            Self::Layout => "Layout",
        }
    }

    /// Every mode, for display and tests.
    pub const ALL: &[Self] = &[Self::Vector, Self::Pixel, Self::Layout];

    /// Tools available in this mode.
    #[must_use]
    pub fn tools(self) -> &'static [PhotoCanvasTool] {
        match self {
            Self::Pixel => &[
                PhotoCanvasTool::Move,
                PhotoCanvasTool::Brush,
                PhotoCanvasTool::Eraser,
                PhotoCanvasTool::Eyedropper,
                PhotoCanvasTool::Pan,
                PhotoCanvasTool::Zoom,
            ],
            Self::Vector | Self::Layout => &[
                PhotoCanvasTool::Move,
                PhotoCanvasTool::Pan,
                PhotoCanvasTool::Zoom,
            ],
        }
    }

    /// True when the tool is available in this mode.
    #[must_use]
    pub fn supports(self, tool: PhotoCanvasTool) -> bool {
        self.tools().contains(&tool)
    }
}

/// Draws the 2-row bar (modes then tool settings).
/// Returns the newly chosen mode, if any (the app falls the tool
/// back when needed via [`PhotoEditMode::supports`]).
pub fn draw_photo_modebar(
    ui: &mut egui::Ui,
    mode: &mut PhotoEditMode,
    tool: PhotoCanvasTool,
    brush: &mut PhotoBrushSettings,
    show_grid: &mut bool,
    theme: &CygnusTheme,
    texts: cygnus_i18n::PhotoCatalog,
) -> Option<PhotoEditMode> {
    let mut chosen = None;
    // Row 1: large mode buttons
    ui.horizontal(|ui| {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            for candidate in PhotoEditMode::ALL {
                let active = *mode == *candidate;
                if Button::new(candidate.label())
                    .variant(if active {
                        ButtonVariant::Primary
                    } else {
                        ButtonVariant::Secondary
                    })
                    .show(ui, theme)
                    .clicked()
                {
                    *mode = *candidate;
                    chosen = Some(*candidate);
                }
            }
        });
    });
    ui.separator();
    // Row 2: selected tool name and settings.
    ui.horizontal(|ui| {
        draw_tool_options(ui, tool, brush, show_grid, theme, texts);
    });
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pixel_mode_supports_every_tool() {
        for tool in [
            PhotoCanvasTool::Move,
            PhotoCanvasTool::Pan,
            PhotoCanvasTool::Zoom,
            PhotoCanvasTool::Brush,
            PhotoCanvasTool::Eraser,
            PhotoCanvasTool::Eyedropper,
        ] {
            assert!(PhotoEditMode::Pixel.supports(tool));
        }
    }

    #[test]
    fn vector_and_layout_are_navigation_only() {
        for mode in [PhotoEditMode::Vector, PhotoEditMode::Layout] {
            assert!(mode.supports(PhotoCanvasTool::Move));
            assert!(!mode.supports(PhotoCanvasTool::Brush));
            assert!(!mode.supports(PhotoCanvasTool::Eraser));
        }
    }

    #[test]
    fn modebar_renders_without_panic_and_idle() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let theme = cygnus_ui::theme::CygnusTheme::dark();
        let texts = cygnus_i18n::PhotoCatalog::new(cygnus_i18n::Language::Fr);
        let mut mode = PhotoEditMode::Pixel;
        let mut brush = PhotoBrushSettings::default();
        let mut show_grid = false;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let chosen = draw_photo_modebar(
                    ui,
                    &mut mode,
                    PhotoCanvasTool::Brush,
                    &mut brush,
                    &mut show_grid,
                    &theme,
                    texts,
                );
                assert_eq!(chosen, None);
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(mode, PhotoEditMode::Pixel);
    }
}

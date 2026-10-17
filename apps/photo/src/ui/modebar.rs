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

//! PHOTO personas and context bar (domain widgets).
//!
//! [`draw_persona_switch`]: compact segmented switch (Vector, Pixel,
//! Layout) living in the menu row, Affinity-style — one top bar
//! instead of a dedicated persona row. [`draw_photo_modebar`]: the
//! context bar under it, name and settings of the selected tool
//! (delegated to `super::optionsbar`) with the view toggles
//! right-aligned. The mode restricts
//! the rail tools (paint tools live in Pixel; Vector and
//! Layout expose move, hand, and zoom until their
//! own tools land): switching modes falls back to Move when
//! the current tool does not exist there.

use super::optionsbar::draw_tool_options;
use super::viewport::{PhotoBrushSettings, PhotoCanvasTool};
use cygnus_ui::components::{Button, ButtonSize, ButtonVariant, Toggle};
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
            Self::Vector => &[
                PhotoCanvasTool::Move,
                PhotoCanvasTool::Node,
                PhotoCanvasTool::Pan,
                PhotoCanvasTool::Zoom,
            ],
            Self::Layout => &[
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

/// Context bar outcome.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModeBarOutcome {
    /// Mask toggle pressed (out-of-document content on/off).
    pub toggle_mask: bool,
}

/// Compact persona switch (segmented: active = primary, others
/// ghost). Returns the newly chosen persona, if any.
pub fn draw_persona_switch(
    ui: &mut egui::Ui,
    mode: &mut PhotoEditMode,
    theme: &CygnusTheme,
) -> Option<PhotoEditMode> {
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = theme.spacing.xxs;
        for candidate in PhotoEditMode::ALL {
            let active = *mode == *candidate;
            if Button::new(candidate.label())
                .variant(if active {
                    ButtonVariant::Primary
                } else {
                    ButtonVariant::Ghost
                })
                .size(ButtonSize::Small)
                .show(ui, theme)
                .clicked()
                && !active
            {
                *mode = *candidate;
                chosen = Some(*candidate);
            }
        }
    });
    chosen
}

/// Context bar: selected tool settings on the left, view toggles
/// (out-of-document mask) on the right.
#[allow(clippy::too_many_arguments)]
pub fn draw_photo_modebar(
    ui: &mut egui::Ui,
    tool: PhotoCanvasTool,
    brush: &mut PhotoBrushSettings,
    show_grid: &mut bool,
    preview_clip: bool,
    theme: &CygnusTheme,
    texts: cygnus_i18n::PhotoCatalog,
) -> ModeBarOutcome {
    let mut outcome = ModeBarOutcome::default();
    ui.horizontal(|ui| {
        draw_tool_options(ui, tool, brush, show_grid, theme, texts);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let mut clip = preview_clip;
            Toggle::new(texts.get(cygnus_i18n::PhotoTextKey::MaskView)).show(ui, theme, &mut clip);
            outcome.toggle_mask = clip != preview_clip;
        });
    });
    outcome
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
                assert_eq!(draw_persona_switch(ui, &mut mode, &theme), None);
                let outcome = draw_photo_modebar(
                    ui,
                    PhotoCanvasTool::Brush,
                    &mut brush,
                    &mut show_grid,
                    false,
                    &theme,
                    texts,
                );
                assert_eq!(outcome, ModeBarOutcome::default());
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(mode, PhotoEditMode::Pixel);
    }
}

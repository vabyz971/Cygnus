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

//! Active-tool settings bar of PHOTO (under the menu).
//!
//! Resumes the old interface: brush (size, opacity, color),
//! eraser (size, opacity), other tools (contextual help).
//! Nothing when the tool has no settings — the bar stays empty.

use super::viewport::{PhotoBrushSettings, PhotoCanvasTool};
use cygnus_i18n::{PhotoCatalog, PhotoTextKey};
use cygnus_ui::components::{Slider, Toggle};
use cygnus_ui::theme::CygnusTheme;
use cygnus_ui::theme::typography::body_text;

/// Draws the active tool's settings (updates `brush` and
/// `show_grid` in place, no channel: purely local state).
pub fn draw_tool_options(
    ui: &mut egui::Ui,
    tool: PhotoCanvasTool,
    brush: &mut PhotoBrushSettings,
    show_grid: &mut bool,
    theme: &CygnusTheme,
    texts: PhotoCatalog,
) {
    match tool {
        PhotoCanvasTool::Brush | PhotoCanvasTool::Eraser => {
            let title = if tool == PhotoCanvasTool::Brush {
                texts.get(PhotoTextKey::BrushHint)
            } else {
                texts.get(PhotoTextKey::EraserHint)
            };
            ui.horizontal(|ui| {
                ui.label(body_text(theme, title));
                ui.separator();
                let mut size = brush.radius * 2.0;
                Slider::new(texts.get(PhotoTextKey::SizeLabel), 1.0..=200.0)
                    .show(ui, theme, &mut size);
                brush.radius = (size / 2.0).clamp(0.5, 100.0);
                let mut opacity = brush.opacity;
                Slider::new(texts.get(PhotoTextKey::OpacityLabel), 0.0..=1.0).show(
                    ui,
                    theme,
                    &mut opacity,
                );
                brush.opacity = opacity.clamp(0.0, 1.0);
                if tool == PhotoCanvasTool::Brush {
                    ui.label(body_text(theme, texts.get(PhotoTextKey::ColorLabel)));
                    let mut color = brush.color;
                    egui::color_picker::color_edit_button_srgb(ui, &mut color);
                    brush.color = color;
                }
            });
        }
        PhotoCanvasTool::Move => {
            ui.horizontal(|ui| {
                ui.label(body_text(theme, texts.get(PhotoTextKey::MoveTitle)));
                ui.separator();
                Toggle::new(texts.get(PhotoTextKey::GridLabel)).show(ui, theme, show_grid);
            });
        }
        PhotoCanvasTool::Node => {
            ui.horizontal(|ui| {
                ui.label(body_text(theme, texts.get(PhotoTextKey::NodeHint)));
                ui.separator();
                Toggle::new(texts.get(PhotoTextKey::GridLabel)).show(ui, theme, show_grid);
            });
        }
        PhotoCanvasTool::Pan => {
            ui.horizontal(|ui| {
                ui.label(body_text(theme, texts.get(PhotoTextKey::PanHintText)));
            });
        }
        PhotoCanvasTool::Zoom => {
            ui.horizontal(|ui| {
                ui.label(body_text(theme, texts.get(PhotoTextKey::ZoomHintText)));
            });
        }
        PhotoCanvasTool::Eyedropper => {
            ui.horizontal(|ui| {
                ui.label(body_text(theme, texts.get(PhotoTextKey::EyedropperHint)));
                ui.separator();
                ui.label(body_text(
                    theme,
                    texts.get(PhotoTextKey::EyedropperHintText),
                ));
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn options_render_for_every_tool_without_panic() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        for tool in [
            PhotoCanvasTool::Move,
            PhotoCanvasTool::Node,
            PhotoCanvasTool::Pan,
            PhotoCanvasTool::Zoom,
            PhotoCanvasTool::Brush,
            PhotoCanvasTool::Eraser,
            PhotoCanvasTool::Eyedropper,
        ] {
            let mut brush = PhotoBrushSettings::default();
            let mut show_grid = false;
            let theme = cygnus_ui::theme::CygnusTheme::dark();
            let texts = PhotoCatalog::new(cygnus_i18n::Language::Fr);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    draw_tool_options(ui, tool, &mut brush, &mut show_grid, &theme, texts);
                });
            })
            .drop_without_applying_deltas();
        }
    }
}

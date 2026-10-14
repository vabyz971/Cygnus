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

//! PHOTO tool rail (left panel, domain widget).
//!
//! Vertical icon bar with a tooltip showing each
//! tool's name. Content follows the edit mode (see `super::modebar`:
//! all tools in Pixel, navigation only in Vector/Layout).
//! cygnus-ui-only styling.

use super::modebar::PhotoEditMode;
use super::viewport::PhotoCanvasTool;
use crate::ui::NewVectorShape;
use cygnus_i18n::{PhotoCatalog, PhotoTextKey};
use cygnus_ui::components::{menu_row, menu_style};
use cygnus_ui::icons::{Icon, IconRegistry};
use cygnus_ui::theme::CygnusTheme;

/// Icon + contextual help of a rail tool.
fn tool_icon(tool: PhotoCanvasTool, texts: PhotoCatalog) -> (Icon, &'static str) {
    match tool {
        PhotoCanvasTool::Move => (Icon::MoveTool, texts.get(PhotoTextKey::MoveHint)),
        PhotoCanvasTool::Node => (Icon::Node, texts.get(PhotoTextKey::NodeHint)),
        PhotoCanvasTool::Pan => (Icon::Hand, texts.get(PhotoTextKey::PanHint)),
        PhotoCanvasTool::Zoom => (Icon::Search, texts.get(PhotoTextKey::ZoomHint)),
        PhotoCanvasTool::Brush => (Icon::Brush, texts.get(PhotoTextKey::BrushHint)),
        PhotoCanvasTool::Eraser => (Icon::Eraser, texts.get(PhotoTextKey::EraserHint)),
        PhotoCanvasTool::Eyedropper => (Icon::Eyedropper, texts.get(PhotoTextKey::EyedropperHint)),
    }
}

/// Tool-rail outcome for one frame: tool switch and/or vector
/// shape creation (both bubble up as [`PhotoAction`]s).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ToolRailOutcome {
    /// Newly chosen tool, if any.
    pub tool: Option<PhotoCanvasTool>,
    /// Vector shape to create (worker-sized, centered), if any.
    pub new_shape: Option<NewVectorShape>,
}

/// Draws the compact vertical tool rail (32 px, brush
/// color right after the tools, Affinity-style) for the current
/// `mode` and returns the frame outcome.
///
/// Hover and selection painted with theme tokens
/// (`item_hover`, `item_selected`): buttons are frameless,
/// the background is drawn before the button (behind the icon).
/// The shape menu (vector creation) sits below the tools in every
/// mode — creation is allowed everywhere, personas switch later.
pub fn draw_tool_rail(
    ui: &mut egui::Ui,
    current_tool: &mut PhotoCanvasTool,
    brush_color: &mut [u8; 3],
    mode: PhotoEditMode,
    theme: &CygnusTheme,
    texts: PhotoCatalog,
) -> ToolRailOutcome {
    let mut outcome = ToolRailOutcome::default();
    ui.vertical_centered(|ui| {
        for (index, tool) in mode.tools().iter().enumerate() {
            let tool = *tool;
            let (icon, tip) = tool_icon(tool, texts);
            let size = egui::vec2(theme.sizes.tool_button, theme.sizes.tool_button);
            let (rect, hover) = ui.allocate_exact_size(size, egui::Sense::hover());
            // Appear fade (native egui) toward hover/selection.
            let active = *current_tool == tool;
            let fade = ui.ctx().animate_bool_with_time(
                ui.make_persistent_id(("photo_tool_hover", index)),
                active || hover.hovered(),
                0.12,
            );
            if fade > 0.0 {
                let base = if active {
                    theme.colors.item_selected
                } else {
                    theme.colors.item_hover
                };
                ui.painter()
                    .rect_filled(rect, theme.radius.sm, base.gamma_multiply(fade));
            }
            let response = ui
                .put(
                    rect,
                    egui::Button::new(
                        IconRegistry::new().sized(icon, theme.typography.heading_size),
                    )
                    .frame(false),
                )
                .on_hover_text(tip);
            if response.clicked() {
                *current_tool = tool;
                outcome.tool = Some(tool);
            }
        }
        ui.separator();
        // Vector creation lives in the Vector persona (Affinity
        // parity: shapes belong to Designer, not Photo). Other
        // personas show tools only.
        if mode == PhotoEditMode::Vector {
            // Vector shape creation (same button visuals, no tool switch).
            let size = egui::vec2(theme.sizes.tool_button, theme.sizes.tool_button);
            let (rect, hover) = ui.allocate_exact_size(size, egui::Sense::hover());
            let fade = ui.ctx().animate_bool_with_time(
                ui.make_persistent_id("photo_shape_menu"),
                hover.hovered(),
                0.12,
            );
            if fade > 0.0 {
                ui.painter().rect_filled(
                    rect,
                    theme.radius.sm,
                    theme.colors.item_hover.gamma_multiply(fade),
                );
            }
            let shape_resp = ui
                .put(
                    rect,
                    egui::Button::new(
                        IconRegistry::new().sized(Icon::Shape, theme.typography.heading_size),
                    )
                    .frame(false),
                )
                .on_hover_text(texts.get(PhotoTextKey::NewVectorLayer));
            egui::Popup::menu(&shape_resp)
                .style(menu_style(theme))
                .show(|ui| {
                    for (label, shape) in [
                        (texts.get(PhotoTextKey::ShapeRect), NewVectorShape::Rect),
                        (
                            texts.get(PhotoTextKey::ShapeEllipse),
                            NewVectorShape::Ellipse,
                        ),
                        (
                            texts.get(PhotoTextKey::ShapePolygon),
                            NewVectorShape::Polygon,
                        ),
                        (texts.get(PhotoTextKey::ShapeStar), NewVectorShape::Star),
                    ] {
                        if menu_row(ui, theme, label) {
                            outcome.new_shape = Some(shape);
                            ui.close();
                        }
                    }
                });
            ui.separator();
        }
        egui::color_picker::color_edit_button_srgb(ui, brush_color);
    });
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_rail_renders_without_panic_and_idle() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let theme = CygnusTheme::dark();
        let texts = PhotoCatalog::new(cygnus_i18n::Language::Fr);
        let mut tool = PhotoCanvasTool::Move;
        let mut color = [255u8, 255, 255];
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert_eq!(
                    draw_tool_rail(
                        ui,
                        &mut tool,
                        &mut color,
                        PhotoEditMode::Pixel,
                        &theme,
                        texts
                    ),
                    ToolRailOutcome::default()
                );
            });
        })
        .drop_without_applying_deltas();
        assert_eq!(tool, PhotoCanvasTool::Move);
    }

    #[test]
    fn every_mode_rail_renders() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let theme = CygnusTheme::dark();
        let texts = PhotoCatalog::new(cygnus_i18n::Language::Fr);
        for mode in [
            PhotoEditMode::Vector,
            PhotoEditMode::Pixel,
            PhotoEditMode::Layout,
        ] {
            let mut tool = PhotoCanvasTool::Move;
            let mut color = [255u8, 255, 255];
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = draw_tool_rail(ui, &mut tool, &mut color, mode, &theme, texts);
                });
            })
            .drop_without_applying_deltas();
        }
    }
}

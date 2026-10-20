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
//! tool's name, color swatches underneath. Content follows the edit mode (see `super::modebar`:
//! all tools in Pixel, navigation only in Vector/Layout).
//! cygnus-ui-only styling.

use super::modebar::PhotoEditMode;
use super::viewport::PhotoBrushSettings;
use super::viewport::PhotoCanvasTool;
use crate::ui::NewVectorShape;
use cygnus_i18n::{PhotoCatalog, PhotoTextKey};
use cygnus_ui::components::{IconButton, menu_row, menu_style};
use cygnus_ui::icons::Icon;
use cygnus_ui::theme::{CygnusTheme, user_color};

/// Icon + contextual help of a rail tool.
pub(crate) fn tool_icon(tool: PhotoCanvasTool, texts: PhotoCatalog) -> (Icon, &'static str) {
    match tool {
        PhotoCanvasTool::Move => (Icon::MoveTool, texts.get(PhotoTextKey::MoveHint)),
        PhotoCanvasTool::Node => (Icon::Node, texts.get(PhotoTextKey::NodeHint)),
        PhotoCanvasTool::Select => (Icon::Marquee, texts.get(PhotoTextKey::SelectHint)),
        PhotoCanvasTool::Lasso => (Icon::Lasso, texts.get(PhotoTextKey::LassoHint)),
        PhotoCanvasTool::Wand => (Icon::Wand, texts.get(PhotoTextKey::WandHint)),
        PhotoCanvasTool::Bucket => (Icon::PaintBucket, texts.get(PhotoTextKey::BucketHint)),
        PhotoCanvasTool::Gradient => (Icon::Gradient, texts.get(PhotoTextKey::GradientHint)),
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

/// One color swatch of the rail (click = picker popup).
fn swatch(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    rect: egui::Rect,
    color: &mut [u8; 3],
    tip: &str,
) {
    let id = ui.make_persistent_id(("photo_swatch", tip));
    let response = ui
        .interact(rect, id, egui::Sense::click())
        .on_hover_text(tip);
    let fill = user_color(*color);
    let painter = ui.painter();
    painter.rect_filled(rect, theme.radius.xs, fill);
    painter.rect_stroke(
        rect,
        theme.radius.xs,
        egui::Stroke::new(theme.borders.thin, theme.colors.fg_subdued),
        egui::StrokeKind::Outside,
    );
    egui::Popup::menu(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| {
            let mut picked = fill;
            if egui::color_picker::color_picker_color32(
                ui,
                &mut picked,
                egui::color_picker::Alpha::Opaque,
            ) {
                *color = [picked.r(), picked.g(), picked.b()];
            }
        });
}

/// Primary/secondary swatches (overlapping squares, Photoshop-style)
/// and the swap button under them.
fn color_swatches(
    ui: &mut egui::Ui,
    brush: &mut PhotoBrushSettings,
    theme: &CygnusTheme,
    texts: PhotoCatalog,
) {
    let side = theme.sizes.button_xs + theme.spacing.xxs;
    let offset = theme.spacing.sm;
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(side + offset, side + offset),
        egui::Sense::hover(),
    );
    let back = egui::Rect::from_min_size(
        rect.min + egui::vec2(offset, offset),
        egui::vec2(side, side),
    );
    let front = egui::Rect::from_min_size(rect.min, egui::vec2(side, side));
    // Secondary first: the primary overlaps it (and wins the clicks).
    swatch(
        ui,
        theme,
        back,
        &mut brush.secondary,
        texts.get(PhotoTextKey::SecondaryColor),
    );
    swatch(
        ui,
        theme,
        front,
        &mut brush.color,
        texts.get(PhotoTextKey::PrimaryColor),
    );
    if IconButton::new(Icon::Swap)
        .tooltip(texts.get(PhotoTextKey::SwapColors))
        .show(ui, theme)
        .clicked()
    {
        std::mem::swap(&mut brush.color, &mut brush.secondary);
    }
}

/// Draws the vertical tool rail for the current `mode` and returns
/// the frame outcome.
///
/// Icon buttons (accent glyph on a pill when selected, theme hover),
/// then the vector shape menu (Vector persona only) and the color
/// swatches.
pub fn draw_tool_rail(
    ui: &mut egui::Ui,
    current_tool: &mut PhotoCanvasTool,
    brush: &mut PhotoBrushSettings,
    mode: PhotoEditMode,
    theme: &CygnusTheme,
    texts: PhotoCatalog,
) -> ToolRailOutcome {
    let mut outcome = ToolRailOutcome::default();
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = theme.spacing.xs;
        ui.add_space(theme.spacing.sm);
        for tool in mode.tools() {
            let tool = *tool;
            let (icon, tip) = tool_icon(tool, texts);
            let response = IconButton::new(icon)
                .tooltip(tip)
                .selected(*current_tool == tool)
                .size(theme.sizes.tool_button)
                .show(ui, theme);
            if response.clicked() {
                *current_tool = tool;
                outcome.tool = Some(tool);
            }
        }
        // Vector creation lives in the Vector persona (Affinity
        // parity: shapes belong to Designer, not Photo).
        if mode == PhotoEditMode::Vector {
            ui.separator();
            let shape_resp = IconButton::new(Icon::Shape)
                .tooltip(texts.get(PhotoTextKey::NewVectorLayer))
                .size(theme.sizes.tool_button)
                .show(ui, theme);
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
        }
        ui.add_space(theme.spacing.md);
        color_swatches(ui, brush, theme, texts);
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
        let mut brush = PhotoBrushSettings::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                assert_eq!(
                    draw_tool_rail(
                        ui,
                        &mut tool,
                        &mut brush,
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
            let mut brush = PhotoBrushSettings::default();
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = draw_tool_rail(ui, &mut tool, &mut brush, mode, &theme, texts);
                });
            })
            .drop_without_applying_deltas();
        }
    }
}

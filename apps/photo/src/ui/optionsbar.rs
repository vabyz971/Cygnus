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

//! Active-tool settings of PHOTO (context bar, after the tool
//! switcher).
//!
//! Brush and eraser: compact size / opacity fields (drag to scrub,
//! click for a slider). Other tools carry no settings yet: their
//! usage hint stands in, subdued.

use super::viewport::{PhotoBrushSettings, PhotoCanvasTool};
use cygnus_i18n::{PhotoCatalog, PhotoTextKey};
use cygnus_ui::components::{Checkbox, IconButton, ValueField};
use cygnus_ui::icons::Icon;
use cygnus_ui::theme::CygnusTheme;
use cygnus_ui::theme::typography::caption_text;
use photo_engine::SelectionKind;

/// Largest brush diameter offered by the size field (image pixels).
const MAX_BRUSH_SIZE: f32 = 500.0;

/// Per-tool settings edited by the context bar (local UI state).
pub struct ToolOptions<'a> {
    /// Brush / eraser settings.
    pub brush: &'a mut PhotoBrushSettings,
    /// Marquee shape of the selection tool.
    pub selection_kind: &'a mut SelectionKind,
    /// Magic wand options.
    pub wand: &'a mut crate::state::WandSettings,
    /// Paint bucket options.
    pub bucket: &'a mut crate::state::WandSettings,
}

/// Draws the active tool's settings (updates `options` in place, no
/// channel: purely local state).
pub fn draw_tool_options(
    ui: &mut egui::Ui,
    tool: PhotoCanvasTool,
    options: ToolOptions<'_>,
    theme: &CygnusTheme,
    texts: PhotoCatalog,
) {
    let ToolOptions {
        brush,
        selection_kind,
        wand,
        bucket,
    } = options;
    match tool {
        PhotoCanvasTool::Bucket | PhotoCanvasTool::Gradient => {
            ui.spacing_mut().item_spacing.x = theme.spacing.sm;
            if tool == PhotoCanvasTool::Bucket {
                ValueField::new(texts.get(PhotoTextKey::ToleranceLabel), 0.0..=255.0).show(
                    ui,
                    theme,
                    &mut bucket.tolerance,
                );
                bucket.tolerance = bucket.tolerance.clamp(0.0, 255.0);
                ui.add_space(theme.spacing.sm);
                Checkbox::new(texts.get(PhotoTextKey::ContiguousLabel)).show(
                    ui,
                    theme,
                    &mut bucket.contiguous,
                );
                ui.add_space(theme.spacing.sm);
            }
            let mut opacity = brush.opacity;
            ValueField::new(texts.get(PhotoTextKey::OpacityLabel), 0.0..=1.0)
                .percent()
                .show(ui, theme, &mut opacity);
            brush.opacity = opacity.clamp(0.0, 1.0);
            ui.add_space(theme.spacing.sm);
            ui.label(caption_text(theme, tool.hint(texts)));
        }
        PhotoCanvasTool::Wand => {
            ui.spacing_mut().item_spacing.x = theme.spacing.sm;
            ValueField::new(texts.get(PhotoTextKey::ToleranceLabel), 0.0..=255.0).show(
                ui,
                theme,
                &mut wand.tolerance,
            );
            wand.tolerance = wand.tolerance.clamp(0.0, 255.0);
            ui.add_space(theme.spacing.sm);
            Checkbox::new(texts.get(PhotoTextKey::ContiguousLabel)).show(
                ui,
                theme,
                &mut wand.contiguous,
            );
            ui.add_space(theme.spacing.sm);
            ui.label(caption_text(theme, tool.hint(texts)));
        }
        PhotoCanvasTool::Select => {
            for (kind, icon, label) in [
                (
                    SelectionKind::Rect,
                    Icon::Marquee,
                    texts.get(PhotoTextKey::SelectionRect),
                ),
                (
                    SelectionKind::Ellipse,
                    Icon::EllipseOutline,
                    texts.get(PhotoTextKey::SelectionEllipse),
                ),
            ] {
                if IconButton::new(icon)
                    .tooltip(label)
                    .selected(*selection_kind == kind)
                    .show(ui, theme)
                    .clicked()
                {
                    *selection_kind = kind;
                }
            }
            ui.add_space(theme.spacing.sm);
            ui.label(caption_text(theme, tool.hint(texts)));
        }
        PhotoCanvasTool::Brush | PhotoCanvasTool::Eraser => {
            ui.spacing_mut().item_spacing.x = theme.spacing.sm;
            let mut size = brush.radius * 2.0;
            ValueField::new(texts.get(PhotoTextKey::SizeLabel), 1.0..=MAX_BRUSH_SIZE)
                .unit("px")
                .show(ui, theme, &mut size);
            brush.radius = (size / 2.0).clamp(0.5, MAX_BRUSH_SIZE / 2.0);
            ui.add_space(theme.spacing.sm);
            let mut opacity = brush.opacity;
            ValueField::new(texts.get(PhotoTextKey::OpacityLabel), 0.0..=1.0)
                .percent()
                .show(ui, theme, &mut opacity);
            brush.opacity = opacity.clamp(0.0, 1.0);
            // Brush v2 tip: hardness, flow, spacing, then the stabilizer.
            for (key, value, range) in [
                (PhotoTextKey::HardnessLabel, &mut brush.hardness, 0.0..=1.0),
                (PhotoTextKey::FlowLabel, &mut brush.flow, 0.01..=1.0),
                (PhotoTextKey::SpacingLabel, &mut brush.spacing, 0.01..=2.0),
                (
                    PhotoTextKey::StabilizerLabel,
                    &mut brush.stabilizer,
                    0.0..=1.0,
                ),
            ] {
                ui.add_space(theme.spacing.sm);
                let (lo, hi) = (*range.start(), *range.end());
                ValueField::new(texts.get(key), range)
                    .percent()
                    .show(ui, theme, value);
                *value = value.clamp(lo, hi);
            }
        }
        other => {
            ui.label(caption_text(theme, other.hint(texts)));
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
            PhotoCanvasTool::Select,
            PhotoCanvasTool::Node,
            PhotoCanvasTool::Lasso,
            PhotoCanvasTool::Wand,
            PhotoCanvasTool::Bucket,
            PhotoCanvasTool::Gradient,
            PhotoCanvasTool::Pan,
            PhotoCanvasTool::Zoom,
            PhotoCanvasTool::Brush,
            PhotoCanvasTool::Eraser,
            PhotoCanvasTool::Eyedropper,
        ] {
            let mut brush = PhotoBrushSettings::default();
            let mut kind = SelectionKind::default();
            let mut wand = crate::state::WandSettings::default();
            let mut bucket = crate::state::WandSettings::default();
            let before = brush;
            let theme = cygnus_ui::theme::CygnusTheme::dark();
            let texts = PhotoCatalog::new(cygnus_i18n::Language::Fr);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let options = ToolOptions {
                            brush: &mut brush,
                            selection_kind: &mut kind,
                            wand: &mut wand,
                            bucket: &mut bucket,
                        };
                        draw_tool_options(ui, tool, options, &theme, texts)
                    });
                });
            })
            .drop_without_applying_deltas();
            assert_eq!(brush, before, "aucun changement sans interaction");
        }
    }
}

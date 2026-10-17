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

//! Histogram panel: RGB (three overlaid channels) or luminance view.

use std::sync::Arc;

use cygnus_i18n::PhotoTextKey;
use cygnus_ui::components::Tabs;
use cygnus_ui::primitives::Text;
use cygnus_ui::theme::CygnusTheme;
use photo_engine::{Histogram, RenderRevision};

use crate::commands::PhotoUiContext;
use crate::ui::PreviewImage;

/// Histogram cached for one render revision (egui temp memory).
#[derive(Clone)]
struct Cached {
    revision: Option<RenderRevision>,
    histogram: Arc<Histogram>,
}

/// Displayed channels.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum View {
    #[default]
    Rgb,
    Luma,
}

/// Bar height for a bin (square-root scale: low counts stay visible
/// next to a dominant peak).
fn bar_height(count: u32, peak: u32, height: f32) -> f32 {
    if peak == 0 {
        return 0.0;
    }
    (count as f32 / peak as f32).sqrt() * height
}

/// Histogram of `preview`, recomputed only when `revision` changes.
fn cached_histogram(
    ui: &egui::Ui,
    salt: impl std::hash::Hash + std::fmt::Debug,
    preview: &PreviewImage,
    revision: Option<RenderRevision>,
) -> Arc<Histogram> {
    let id = egui::Id::new(("photo_histogram", salt));
    if let Some(hit) = ui.data(|d| d.get_temp::<Cached>(id))
        && hit.revision == revision
    {
        return hit.histogram;
    }
    let histogram = Arc::new(Histogram::from_rgba(&preview.rgba));
    ui.data_mut(|d| {
        d.insert_temp(
            id,
            Cached {
                revision,
                histogram: Arc::clone(&histogram),
            },
        );
    });
    histogram
}

/// Plots the bins of the visible channels.
fn plot(ui: &mut egui::Ui, theme: &CygnusTheme, histogram: &Histogram, view: View) {
    let width = ui.available_width().max(theme.sizes.preset_card);
    let height = (width * 0.5).max(theme.sizes.preset_card * 0.5);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme.radius.sm, theme.colors.bg_tertiary);
    let channels: Vec<(&[u32; 256], egui::Color32)> = match view {
        View::Rgb => vec![
            (&histogram.red, theme.colors.channel_red.gamma_multiply(0.6)),
            (
                &histogram.green,
                theme.colors.channel_green.gamma_multiply(0.6),
            ),
            (
                &histogram.blue,
                theme.colors.channel_blue.gamma_multiply(0.6),
            ),
        ],
        View::Luma => vec![(&histogram.luma, theme.colors.fg_secondary)],
    };
    let peak = match view {
        View::Rgb => histogram
            .red
            .iter()
            .chain(histogram.green.iter())
            .chain(histogram.blue.iter())
            .copied()
            .max()
            .unwrap_or(0),
        View::Luma => histogram.luma.iter().copied().max().unwrap_or(0),
    };
    let bin_w = rect.width() / 256.0;
    for (bins, color) in channels {
        let stroke = egui::Stroke::new(bin_w.max(theme.borders.thin), color);
        for (i, count) in bins.iter().enumerate() {
            let h = bar_height(*count, peak, rect.height());
            if h <= 0.0 {
                continue;
            }
            let x = rect.left() + (i as f32 + 0.5) * bin_w;
            painter.line_segment(
                [
                    egui::pos2(x, rect.bottom()),
                    egui::pos2(x, rect.bottom() - h),
                ],
                stroke,
            );
        }
    }
    painter.rect_stroke(
        rect,
        theme.radius.sm,
        egui::Stroke::new(theme.borders.thin, theme.colors.border),
        egui::StrokeKind::Inside,
    );
}

/// Histogram panel (direct content, title carried by the dock tab).
pub struct HistogramPanel;

impl HistogramPanel {
    /// Draws the histogram of `preview` (`None` = empty document).
    /// `salt` identifies the document (one cache per document).
    pub fn show(
        ui: &mut egui::Ui,
        ctx: &PhotoUiContext,
        salt: impl std::hash::Hash + std::fmt::Debug + Copy,
        preview: Option<&PreviewImage>,
        revision: Option<RenderRevision>,
    ) {
        let theme = ctx.shared.theme();
        let texts = cygnus_i18n::PhotoCatalog::new(ctx.shared.translator().language());
        let Some(preview) = preview else {
            Text::body(theme, texts.get(PhotoTextKey::HistogramEmpty)).show(ui);
            return;
        };
        let view_id = egui::Id::new(("photo_histogram_view", salt));
        let mut index = match ui.data(|d| d.get_temp::<View>(view_id)).unwrap_or_default() {
            View::Rgb => 0,
            View::Luma => 1,
        };
        let labels = [
            texts.get(PhotoTextKey::HistogramRgb),
            texts.get(PhotoTextKey::HistogramLuma),
        ];
        Tabs::new(&labels).show(ui, theme, &mut index);
        let view = if index == 1 { View::Luma } else { View::Rgb };
        ui.data_mut(|d| d.insert_temp(view_id, view));
        let histogram = cached_histogram(ui, salt, preview, revision);
        if histogram.samples == 0 {
            Text::body(theme, texts.get(PhotoTextKey::HistogramEmpty)).show(ui);
            return;
        }
        plot(ui, theme, &histogram, view);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn preview() -> PreviewImage {
        PreviewImage {
            width: 2,
            height: 1,
            rgba: vec![255, 0, 0, 255, 0, 0, 255, 255],
            full_width: 2,
            full_height: 1,
            origin_x: 0.0,
            origin_y: 0.0,
            doc_width: 2,
            doc_height: 1,
        }
    }

    #[test]
    fn sqrt_scale_bounds() {
        assert_eq!(bar_height(0, 0, 100.0), 0.0);
        assert_eq!(bar_height(4, 4, 100.0), 100.0);
        assert!((bar_height(1, 4, 100.0) - 50.0).abs() < 1e-3);
    }

    #[test]
    fn cache_follows_revision() {
        let ctx = egui::Context::default();
        let mut first = None;
        let mut same = None;
        let mut fresh = None;
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = preview();
            first = Some(cached_histogram(ui, 1u8, &p, Some(RenderRevision(1))));
            same = Some(cached_histogram(ui, 1u8, &p, Some(RenderRevision(1))));
            fresh = Some(cached_histogram(ui, 1u8, &p, Some(RenderRevision(2))));
        })
        .drop_without_applying_deltas();
        let (Some(first), Some(same), Some(fresh)) = (first, same, fresh) else {
            panic!("histogrammes attendus");
        };
        assert!(Arc::ptr_eq(&first, &same), "même révision : cache");
        assert!(!Arc::ptr_eq(&first, &fresh), "nouvelle révision : recalcul");
        assert_eq!(first.red[255], 1);
        assert_eq!(first.blue[255], 1);
    }

    #[test]
    fn panel_renders_with_and_without_preview() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let photo_ctx = PhotoUiContext::for_frame(&ctx);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let p = preview();
                HistogramPanel::show(ui, &photo_ctx, 7u8, Some(&p), Some(RenderRevision(3)));
                HistogramPanel::show(ui, &photo_ctx, 8u8, None, None);
            });
        })
        .drop_without_applying_deltas();
    }
}

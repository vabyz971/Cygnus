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

//! Histogram panel: RGB (three overlaid filled channels) or luminance
//! view, axis legend and luma statistics.

use std::sync::Arc;

use cygnus_i18n::{PhotoTextKey, TextKey};
use cygnus_ui::containers::section_header;
use cygnus_ui::theme::CygnusTheme;
use cygnus_ui::theme::typography::caption_text;
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

/// Bins lightly smoothed (5-tap box) so the plot reads as a curve.
fn smoothed(bins: &[u32; 256]) -> [f32; 256] {
    let mut out = [0.0; 256];
    for (i, slot) in out.iter_mut().enumerate() {
        let lo = i.saturating_sub(2);
        let hi = (i + 2).min(255);
        let sum: u32 = bins[lo..=hi].iter().sum();
        *slot = sum as f32 / (hi - lo + 1) as f32;
    }
    out
}

/// Filled area of one channel (vertical quads from the baseline) and
/// its brighter outline.
fn paint_channel(
    painter: &egui::Painter,
    rect: egui::Rect,
    bins: &[u32; 256],
    peak: f32,
    color: egui::Color32,
    line: egui::Stroke,
) {
    let values = smoothed(bins);
    let x_at = |i: usize| rect.left() + i as f32 / 255.0 * rect.width();
    let y_at = |v: f32| rect.bottom() - bar_height(v.round() as u32, peak as u32, rect.height());
    let fill = color.gamma_multiply(0.45);
    let mut mesh = egui::Mesh::default();
    let mut outline = Vec::with_capacity(256);
    for (i, value) in values.iter().enumerate() {
        let top = egui::pos2(x_at(i), y_at(*value));
        outline.push(top);
        mesh.colored_vertex(egui::pos2(top.x, rect.bottom()), fill);
        mesh.colored_vertex(top, fill);
        if i > 0 {
            let base = (i as u32 - 1) * 2;
            mesh.add_triangle(base, base + 1, base + 2);
            mesh.add_triangle(base + 1, base + 3, base + 2);
        }
    }
    painter.add(mesh);
    painter.add(egui::Shape::line(outline, line));
}

/// Plots the visible channels as overlapping filled curves.
fn plot(ui: &mut egui::Ui, theme: &CygnusTheme, histogram: &Histogram, view: View) {
    let width = ui.available_width().max(theme.sizes.preset_card);
    let height = (width * 0.32).max(theme.sizes.preset_card * 0.6);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, theme.radius.sm, theme.colors.bg_tertiary);
    let plot_rect = rect.shrink(theme.spacing.xxs);
    let channels: Vec<(&[u32; 256], egui::Color32)> = match view {
        View::Rgb => vec![
            (&histogram.blue, theme.colors.channel_blue),
            (&histogram.green, theme.colors.channel_green),
            (&histogram.red, theme.colors.channel_red),
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
    for (bins, color) in channels {
        paint_channel(
            &painter,
            plot_rect,
            bins,
            peak as f32,
            color,
            egui::Stroke::new(theme.borders.thin, color.gamma_multiply(0.9)),
        );
    }
}

/// Decimal number in the interface language (comma in French).
fn decimal(value: f32, language: cygnus_i18n::Language) -> String {
    let text = format!("{value:.1}");
    match language {
        cygnus_i18n::Language::Fr => text.replace('.', ","),
        cygnus_i18n::Language::En => text,
    }
}

/// "0 / midtones / 255" axis under the plot, then the luma mean and
/// standard deviation.
fn legend(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    histogram: &Histogram,
    texts: cygnus_i18n::PhotoCatalog,
    language: cygnus_i18n::Language,
) {
    // Axis: one row, three anchors (left, center, right).
    let font = egui::FontId::proportional(theme.typography.caption_size);
    let row = ui.fonts_mut(|fonts| fonts.row_height(&font));
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), row), egui::Sense::hover());
    let painter = ui.painter();
    for (anchor, pos, text) in [
        (egui::Align2::LEFT_CENTER, rect.left_center(), "0"),
        (
            egui::Align2::CENTER_CENTER,
            rect.center(),
            texts.get(PhotoTextKey::HistogramMidtones),
        ),
        (egui::Align2::RIGHT_CENTER, rect.right_center(), "255"),
    ] {
        painter.text(pos, anchor, text, font.clone(), theme.colors.fg_subdued);
    }
    let Some((mean, deviation)) = histogram.luma_stats() else {
        return;
    };
    let stat = |label: &str, value: f32| format!("{label} {}", decimal(value, language));
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(stat(texts.get(PhotoTextKey::HistogramMean), mean))
                .size(theme.typography.caption_size)
                .color(theme.colors.fg_secondary),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(
                egui::RichText::new(stat(texts.get(PhotoTextKey::HistogramStdDev), deviation))
                    .size(theme.typography.caption_size)
                    .color(theme.colors.fg_secondary),
            );
        });
    });
}

/// Histogram panel (section header + plot + legend).
pub struct HistogramPanel;

impl HistogramPanel {
    /// Draws the histogram of `preview` (`None` = empty document).
    /// `salt` identifies the document (one cache per document);
    /// `menu` fills the header's overflow slot (right-most).
    pub fn show(
        ui: &mut egui::Ui,
        ctx: &PhotoUiContext,
        salt: impl std::hash::Hash + std::fmt::Debug + Copy,
        preview: Option<&PreviewImage>,
        revision: Option<RenderRevision>,
        menu: impl FnOnce(&mut egui::Ui),
    ) {
        let theme = ctx.shared.theme();
        let language = ctx.shared.translator().language();
        let texts = cygnus_i18n::PhotoCatalog::new(language);
        let view_id = egui::Id::new(("photo_histogram_view", salt));
        let mut view = ui.data(|d| d.get_temp::<View>(view_id)).unwrap_or_default();
        section_header(
            ui,
            theme,
            ctx.shared.translator().get(TextKey::Histogram),
            |ui| {
                menu(ui);
                // Channel switch: click cycles RGB <-> luma.
                let label = match view {
                    View::Rgb => texts.get(PhotoTextKey::HistogramRgb),
                    View::Luma => texts.get(PhotoTextKey::HistogramLuma),
                };
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new(label)
                                .size(theme.typography.caption_size)
                                .color(theme.colors.fg_secondary),
                        )
                        .frame_when_inactive(false),
                    )
                    .clicked()
                {
                    view = match view {
                        View::Rgb => View::Luma,
                        View::Luma => View::Rgb,
                    };
                }
            },
        );
        ui.data_mut(|d| d.insert_temp(view_id, view));
        let Some(preview) = preview else {
            ui.label(caption_text(theme, texts.get(PhotoTextKey::HistogramEmpty)));
            return;
        };
        let histogram = cached_histogram(ui, salt, preview, revision);
        if histogram.samples == 0 {
            ui.label(caption_text(theme, texts.get(PhotoTextKey::HistogramEmpty)));
            return;
        }
        plot(ui, theme, &histogram, view);
        legend(ui, theme, &histogram, texts, language);
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
                HistogramPanel::show(
                    ui,
                    &photo_ctx,
                    7u8,
                    Some(&p),
                    Some(RenderRevision(3)),
                    |_| {},
                );
                HistogramPanel::show(ui, &photo_ctx, 8u8, None, None, |_| {});
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn decimals_follow_the_language() {
        assert_eq!(decimal(112.84, cygnus_i18n::Language::Fr), "112,8");
        assert_eq!(decimal(48.25, cygnus_i18n::Language::En), "48.2");
    }

    #[test]
    fn smoothing_keeps_flat_bins_flat() {
        let bins = [4u32; 256];
        assert!(smoothed(&bins).iter().all(|v| (*v - 4.0).abs() < 1e-6));
    }
}

// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! History and Histogram panels (HIS-3, REG-4, docs/decisions/0022 and
//! 0025). The history lists the entries of the bus with their time and
//! their source (user or agent); clicking an entry undoes or redoes up to
//! it.

use std::time::SystemTime;

use cygnus_commands::{Entry, Source};
use cygnus_photo_core::PhotoCommand;
use cygnus_photo_render::Histogram;
use cygnus_ui_kit::i18n::{Key, tr, tr_format};
use cygnus_ui_kit::icons;
use cygnus_ui_kit::theme::{metrics, palette};
use cygnus_ui_kit::widgets;
use egui::{Align, Color32, CornerRadius, Frame, Layout, Margin, RichText, Sense, Ui, pos2, vec2};

use crate::canvas::Session;
use crate::keys;

/// Translation key of a command name of the catalog.
pub fn command_key(name: &str) -> Key {
    match name {
        "document.create" => keys::COMMAND_DOCUMENT_CREATE,
        "layer.create" => keys::COMMAND_LAYER_CREATE,
        "layer.update" => keys::COMMAND_LAYER_UPDATE,
        "layer.delete" => keys::COMMAND_LAYER_DELETE,
        "layer.duplicate" => keys::COMMAND_LAYER_DUPLICATE,
        "layer.move" => keys::COMMAND_LAYER_MOVE,
        "layer.group" => keys::COMMAND_LAYER_GROUP,
        "layer.ungroup" => keys::COMMAND_LAYER_UNGROUP,
        "adjustment.update" => keys::COMMAND_ADJUSTMENT_UPDATE,
        "filter.update" => keys::COMMAND_FILTER_UPDATE,
        "mask.create" => keys::COMMAND_MASK_CREATE,
        "mask.delete" => keys::COMMAND_MASK_DELETE,
        "mask.update" => keys::COMMAND_MASK_UPDATE,
        "mask.fill" => keys::COMMAND_MASK_FILL,
        "mask.gradient" => keys::COMMAND_MASK_GRADIENT,
        "source.import" => keys::COMMAND_SOURCE_IMPORT,
        "layer.merge_down" => keys::COMMAND_LAYER_MERGE_DOWN,
        "pixels.fill" | "pixels.write" => keys::COMMAND_PIXELS,
        _ => keys::COMMAND_OTHER,
    }
}

/// Icon and translation key of who issued an entry (HIS-3).
pub fn source_mark(source: Source) -> (&'static str, Key) {
    match source {
        Source::User => (icons::USER, keys::SOURCE_USER),
        Source::Agent => (icons::ROBOT, keys::SOURCE_AGENT),
        Source::Cli => (icons::TERMINAL, keys::SOURCE_CLI),
        Source::Test => (icons::FILE_CODE, keys::SOURCE_TEST),
    }
}

/// Local time of `time`, `HH:MM:SS`.
pub fn local_time(time: SystemTime) -> String {
    jiff::Timestamp::try_from(time)
        .map(|stamp| {
            stamp
                .to_zoned(jiff::tz::TimeZone::system())
                .strftime("%H:%M:%S")
                .to_string()
        })
        .unwrap_or_default()
}

fn entry_row(ui: &mut Ui, entry: &Entry<PhotoCommand>, current: bool, undone: bool) -> bool {
    let (icon, source) = source_mark(entry.source);
    let text = if undone {
        palette::TEXT_DISABLED
    } else {
        palette::TEXT
    };
    let row = Frame::new()
        .fill(if current {
            palette::SELECTED
        } else {
            Color32::TRANSPARENT
        })
        .corner_radius(CornerRadius::same(metrics::RADIUS))
        .inner_margin(Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(widgets::icon(icon, metrics::ICON - 2.0).color(palette::TEXT_MUTED))
                    .on_hover_text(tr(source));
                ui.label(RichText::new(tr(command_key(entry.name))).color(text));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.label(
                        RichText::new(local_time(entry.time))
                            .monospace()
                            .color(palette::TEXT_MUTED),
                    );
                    ui.label(RichText::new(tr(source)).small().color(palette::TEXT_MUTED));
                });
            });
        });
    row.response.interact(Sense::click()).clicked()
}

/// The History panel.
pub fn panel(ui: &mut Ui, session: &mut Session) {
    let done = session.bus().entries().count();
    let undone = session.bus().undone().count();
    if done + undone == 0 {
        ui.label(RichText::new(tr(keys::HISTORY_EMPTY)).color(palette::TEXT_MUTED));
        return;
    }
    // Target: number of done entries wanted after the click.
    let mut target = None;
    egui::ScrollArea::vertical()
        .stick_to_bottom(true)
        .show(ui, |ui| {
            for (index, entry) in session.bus().entries().enumerate() {
                if entry_row(ui, entry, index + 1 == done, false) {
                    target = Some(index + 1);
                }
            }
            for (index, entry) in session.bus().undone().enumerate() {
                if entry_row(ui, entry, false, true) {
                    target = Some(done + index + 1);
                }
            }
        });
    if let Some(target) = target {
        // Gestures undo as a whole: stop when nothing moves.
        let mut last = usize::MAX;
        loop {
            let now = session.bus().entries().count();
            if now == target || now == last {
                break;
            }
            last = now;
            if now > target {
                session.undo();
            } else {
                session.redo();
            }
        }
    }
}

/// The Histogram panel: R, G and B of the area shown, mean and standard
/// deviation (approximation documented in docs/decisions/0025).
pub fn histogram_panel(ui: &mut Ui, histogram: Option<&Histogram>) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 96.0), Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(metrics::RADIUS), palette::INSET);
    let caption = |ui: &mut Ui, text: &str| {
        ui.label(RichText::new(text).small().color(palette::TEXT_MUTED));
    };
    let Some(histogram) = histogram else {
        caption(ui, tr(keys::HISTOGRAM_WAITING));
        return;
    };
    let peak = histogram
        .bins
        .iter()
        .flat_map(|channel| channel.iter())
        .copied()
        .max()
        .unwrap_or(0)
        .max(1);
    let bin_width = rect.width() / 256.0;
    for (channel, color) in palette::CHANNELS.iter().enumerate() {
        for (bin, count) in histogram.bins[channel].iter().enumerate() {
            if *count == 0 {
                continue;
            }
            // Square root scale, so that small populations stay visible.
            let height = (*count as f32 / peak as f32).sqrt() * rect.height() * 0.95;
            let left = rect.left() + bin as f32 * bin_width;
            painter.rect_filled(
                egui::Rect::from_min_max(
                    pos2(left, rect.bottom() - height),
                    pos2(left + bin_width.max(1.0), rect.bottom()),
                ),
                CornerRadius::ZERO,
                color.gamma_multiply(0.45),
            );
        }
    }
    let (mean, deviation) = histogram.mean_and_std_dev();
    let number = |v: f64| format!("{v:.1}").replace('.', tr(keys::NUMBER_DECIMAL_SEPARATOR));
    ui.horizontal(|ui| {
        caption(ui, "0");
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            caption(ui, "255");
            ui.add_space((ui.available_width() - 60.0) / 2.0);
            caption(ui, tr(keys::HISTOGRAM_AREA));
        });
    });
    ui.horizontal(|ui| {
        caption(
            ui,
            &tr_format(keys::HISTOGRAM_MEAN, &[("value", &number(mean))]),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            caption(
                ui,
                &tr_format(keys::HISTOGRAM_STD_DEV, &[("value", &number(deviation))]),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_of_the_catalog_has_a_name_in_the_history() {
        use cygnus_commands::Catalog;
        for info in PhotoCommand::commands() {
            if info.name.starts_with("history.") {
                continue;
            }
            assert_ne!(
                command_key(info.name),
                keys::COMMAND_OTHER,
                "{} has no history name",
                info.name
            );
        }
    }

    #[test]
    fn agent_and_user_entries_are_told_apart() {
        assert_ne!(source_mark(Source::User), source_mark(Source::Agent));
        assert_eq!(source_mark(Source::Agent).1, keys::SOURCE_AGENT);
    }
}

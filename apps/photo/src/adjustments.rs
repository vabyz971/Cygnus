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

//! Adjustments panel (REG-2, REG-3, docs/decisions/0025): the parameters of
//! the selected adjustment layer, its intensity and the density of its
//! mask. Every drag is one gesture, so one history entry.

use cygnus_photo_core::adjust::{Adjustment, ColorShift, Curve, LevelsChannel, MAX_CURVE_POINTS};
use cygnus_photo_core::commands::{AdjustmentUpdate, LayerUpdate, MaskUpdate};
use cygnus_photo_core::{NodeId, NodeKind, PhotoCommand};
use cygnus_ui_kit::i18n::{Key, tr};
use cygnus_ui_kit::theme::{metrics, palette};
use cygnus_ui_kit::{icons, widgets};
use egui::{
    Align, CornerRadius, Layout, Pos2, Response, RichText, Sense, Shape, Stroke, Ui, pos2, vec2,
};

use crate::canvas::Session;
use crate::keys;
use crate::layers::adjustment_key;
use crate::tools::percent;
use crate::view::ViewState;

/// Channels of levels and curves: master, red, green, blue.
const CHANNELS: [Key; 4] = [
    keys::HISTOGRAM_RGB,
    keys::CHANNEL_RED,
    keys::CHANNEL_GREEN,
    keys::CHANNEL_BLUE,
];

/// Follows a drag of `response`: opens the gesture when it starts and
/// closes it when it stops.
fn gesture(session: &mut Session, response: &Response) {
    if response.drag_started() {
        session.begin_gesture();
    }
    if response.drag_stopped() {
        session.end_gesture();
    }
}

/// A labeled slider; returns its response.
fn slider(
    ui: &mut Ui,
    label: Key,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    text: String,
) -> Response {
    ui.horizontal(|ui| {
        ui.label(RichText::new(tr(label)).small().color(palette::TEXT_MUTED));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(text).monospace().color(palette::TEXT));
        });
    });
    widgets::slider(ui, value, range)
}

fn channel_combo(ui: &mut Ui, view: &mut ViewState) {
    widgets::labeled_combo(
        ui,
        "adjustment-channel",
        "",
        tr(CHANNELS[view.curve_channel]),
        64.0,
        |ui| {
            for (index, key) in CHANNELS.iter().enumerate() {
                ui.selectable_value(&mut view.curve_channel, index, tr(*key));
            }
        },
    );
}

/// The Adjustments panel.
pub fn panel(ui: &mut Ui, session: &mut Session, view: &mut ViewState) {
    view.follow(session.document());
    let Some(node) = view
        .selected
        .and_then(|id| session.document().node(id))
        .cloned()
    else {
        none(ui);
        return;
    };
    let NodeKind::Adjustment { adjustment } = &node.kind else {
        none(ui);
        return;
    };
    let id = node.id;
    let mut edited = adjustment.clone();
    let mut changed = false;
    ui.horizontal(|ui| {
        let icon = if matches!(edited, Adjustment::Curves(_)) {
            icons::BEZIER_CURVE
        } else {
            icons::SLIDERS
        };
        widgets::icon_label(ui, icon, tr(adjustment_key(&edited)), palette::TEXT);
        if matches!(edited, Adjustment::Levels(_) | Adjustment::Curves(_)) {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                channel_combo(ui, view)
            });
        }
    });
    match &mut edited {
        Adjustment::Levels(levels) => {
            let channel = match view.curve_channel {
                1 => &mut levels.red,
                2 => &mut levels.green,
                3 => &mut levels.blue,
                _ => &mut levels.master,
            };
            changed |= levels_controls(ui, session, channel);
        }
        Adjustment::Curves(curves) => {
            let curve = match view.curve_channel {
                1 => &mut curves.red,
                2 => &mut curves.green,
                3 => &mut curves.blue,
                _ => &mut curves.master,
            };
            changed |= curve_editor(ui, session, view, curve);
        }
        Adjustment::HueSaturation(p) => {
            for (label, value, range) in [
                (keys::HUE, &mut p.hue, -180.0..=180.0),
                (keys::SATURATION, &mut p.saturation, -100.0..=100.0),
                (keys::LIGHTNESS, &mut p.lightness, -100.0..=100.0),
            ] {
                let text = format!("{:+.0}", *value);
                let response = slider(ui, label, value, range, text);
                gesture(session, &response);
                changed |= response.changed();
            }
        }
        Adjustment::BrightnessContrast(p) => {
            for (label, value) in [
                (keys::BRIGHTNESS, &mut p.brightness),
                (keys::CONTRAST, &mut p.contrast),
            ] {
                let text = format!("{:+.0}", *value);
                let response = slider(ui, label, value, -100.0..=100.0, text);
                gesture(session, &response);
                changed |= response.changed();
            }
        }
        Adjustment::ColorBalance(p) => {
            let ranges: [(Key, &mut ColorShift); 3] = [
                (keys::BALANCE_SHADOWS, &mut p.shadows),
                (keys::BALANCE_MIDTONES, &mut p.midtones),
                (keys::BALANCE_HIGHLIGHTS, &mut p.highlights),
            ];
            for (range, shift) in ranges {
                ui.label(RichText::new(tr(range)).color(palette::TEXT));
                for (label, value) in [
                    (keys::BALANCE_CYAN_RED, &mut shift.cyan_red),
                    (keys::BALANCE_MAGENTA_GREEN, &mut shift.magenta_green),
                    (keys::BALANCE_YELLOW_BLUE, &mut shift.yellow_blue),
                ] {
                    let text = format!("{:+.0}", *value);
                    let response = slider(ui, label, value, -100.0..=100.0, text);
                    gesture(session, &response);
                    changed |= response.changed();
                }
            }
            changed |= ui
                .checkbox(&mut p.preserve_luminosity, tr(keys::BALANCE_PRESERVE))
                .changed();
        }
    }
    if changed {
        session.user(PhotoCommand::AdjustmentUpdate(AdjustmentUpdate {
            id,
            adjustment: edited,
        }));
    }

    ui.add_space(metrics::SPACING);
    let mut strength = node.opacity * 100.0;
    let text = percent(strength.round() as u32);
    let response = slider(
        ui,
        keys::ADJUSTMENT_STRENGTH,
        &mut strength,
        0.0..=100.0,
        text,
    );
    gesture(session, &response);
    if response.changed() {
        let mut params = LayerUpdate::new(id);
        params.opacity = Some(strength / 100.0);
        session.user(PhotoCommand::LayerUpdate(params));
    }
    mask_density(ui, session, node.mask);
}

fn none(ui: &mut Ui) {
    ui.label(RichText::new(tr(keys::ADJUSTMENTS_NONE)).color(palette::TEXT_MUTED));
}

/// Density of the mask `mask`, if the layer has one.
fn mask_density(ui: &mut Ui, session: &mut Session, mask: Option<NodeId>) {
    let Some(mask) = mask else { return };
    let Some(NodeKind::Mask { density, .. }) =
        session.document().node(mask).map(|n| n.kind.clone())
    else {
        return;
    };
    let mut value = density * 100.0;
    let text = percent(value.round() as u32);
    let response = slider(
        ui,
        keys::ADJUSTMENT_MASK_DENSITY,
        &mut value,
        0.0..=100.0,
        text,
    );
    gesture(session, &response);
    if response.changed() {
        session.user(PhotoCommand::MaskUpdate(MaskUpdate {
            id: mask,
            density: Some(value / 100.0),
            inverted: None,
            enabled: None,
        }));
    }
}

fn levels_controls(ui: &mut Ui, session: &mut Session, channel: &mut LevelsChannel) -> bool {
    let mut changed = false;
    let level = |v: f32| format!("{:.0}", v * 255.0);
    for (label, value) in [
        (keys::LEVELS_INPUT_BLACK, &mut channel.input_black),
        (keys::LEVELS_INPUT_WHITE, &mut channel.input_white),
        (keys::LEVELS_OUTPUT_BLACK, &mut channel.output_black),
        (keys::LEVELS_OUTPUT_WHITE, &mut channel.output_white),
    ] {
        let text = level(*value);
        let response = slider(ui, label, value, 0.0..=1.0, text);
        gesture(session, &response);
        changed |= response.changed();
    }
    let text = format!("{:.2}", channel.gamma);
    let response = slider(ui, keys::LEVELS_GAMMA, &mut channel.gamma, 0.1..=10.0, text);
    gesture(session, &response);
    changed | response.changed()
}

/// Smallest gap kept between two curve points.
const POINT_GAP: f32 = 0.01;

/// Curve editor: drag a point, click to add one, right-click to remove one.
/// The curve is drawn with `Curve::evaluate`, the function the renderer
/// samples.
fn curve_editor(
    ui: &mut Ui,
    session: &mut Session,
    view: &mut ViewState,
    curve: &mut Curve,
) -> bool {
    let width = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, width.min(200.0)), Sense::click_and_drag());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(metrics::RADIUS), palette::INSET);
    let grid = Stroke::new(1.0, palette::BORDER);
    for step in 1..4 {
        let t = step as f32 / 4.0;
        painter.vline(rect.left() + rect.width() * t, rect.y_range(), grid);
        painter.hline(rect.x_range(), rect.top() + rect.height() * t, grid);
    }
    painter.line_segment(
        [rect.left_bottom(), rect.right_top()],
        Stroke::new(1.0, palette::TEXT_DISABLED),
    );
    let to_screen = |x: f32, y: f32| -> Pos2 {
        pos2(
            rect.left() + x * rect.width(),
            rect.bottom() - y * rect.height(),
        )
    };
    let to_curve = |p: Pos2| -> [f32; 2] {
        [
            ((p.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
            ((rect.bottom() - p.y) / rect.height()).clamp(0.0, 1.0),
        ]
    };
    let line: Vec<Pos2> = (0..=128)
        .map(|i| {
            let x = i as f32 / 128.0;
            to_screen(x, curve.evaluate(x))
        })
        .collect();
    painter.add(Shape::line(line, Stroke::new(1.5, palette::ACCENT)));
    for (i, [x, y]) in curve.points.iter().enumerate() {
        let fill = if view.curve_point == Some(i) {
            palette::ACCENT
        } else {
            palette::INSET
        };
        painter.circle(
            to_screen(*x, *y),
            4.0,
            fill,
            Stroke::new(1.5, palette::ACCENT_TEXT),
        );
    }
    painter.text(
        rect.left_top() + vec2(8.0, 8.0),
        egui::Align2::LEFT_TOP,
        tr(keys::ADJUSTMENT_AXES),
        egui::FontId::monospace(9.5),
        palette::TEXT_MUTED,
    );
    ui.label(
        RichText::new(tr(keys::ADJUSTMENT_CURVES_HINT))
            .small()
            .color(palette::TEXT_MUTED),
    );

    let nearest = |pointer: Pos2, points: &[[f32; 2]]| -> Option<usize> {
        points
            .iter()
            .enumerate()
            .map(|(i, [x, y])| (i, to_screen(*x, *y).distance(pointer)))
            .filter(|(_, d)| *d <= 10.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(i, _)| i)
    };
    let insert = |points: &mut Vec<[f32; 2]>, at: [f32; 2]| -> Option<usize> {
        if points.len() >= MAX_CURVE_POINTS {
            return None;
        }
        let index = points
            .iter()
            .position(|p| p[0] > at[0])
            .unwrap_or(points.len());
        let left_ok = index == 0 || at[0] - points[index - 1][0] >= POINT_GAP;
        let right_ok = index == points.len() || points[index][0] - at[0] >= POINT_GAP;
        (left_ok && right_ok).then(|| {
            points.insert(index, at);
            index
        })
    };
    let mut changed = false;
    if let Some(pointer) = response.interact_pointer_pos() {
        if response.drag_started() {
            session.begin_gesture();
            view.curve_point = nearest(pointer, &curve.points).or_else(|| {
                let added = insert(&mut curve.points, to_curve(pointer));
                changed |= added.is_some();
                added
            });
        } else if response.dragged()
            && let Some(i) = view.curve_point
            && i < curve.points.len()
        {
            let [mut x, y] = to_curve(pointer);
            let low = if i == 0 {
                0.0
            } else {
                curve.points[i - 1][0] + POINT_GAP
            };
            let high = if i + 1 == curve.points.len() {
                1.0
            } else {
                curve.points[i + 1][0] - POINT_GAP
            };
            x = x.clamp(low, high.max(low));
            if curve.points[i] != [x, y] {
                curve.points[i] = [x, y];
                changed = true;
            }
        }
        if response.clicked() && nearest(pointer, &curve.points).is_none() {
            changed |= insert(&mut curve.points, to_curve(pointer)).is_some();
        }
        if response.secondary_clicked()
            && curve.points.len() > 2
            && let Some(i) = nearest(pointer, &curve.points)
        {
            curve.points.remove(i);
            changed = true;
        }
    }
    if response.drag_stopped() {
        session.end_gesture();
        view.curve_point = None;
    }
    changed
}

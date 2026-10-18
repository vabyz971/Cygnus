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

//! Client-side window chrome (the OS title bar is turned off in
//! `main.rs`): drag area, minimize / maximize / close buttons and
//! edge resizing. Everything goes through `ViewportCommand`s, so the
//! compositor still owns the actual move and resize.

use cygnus_ui::components::IconButton;
use cygnus_ui::icons::Icon;
use cygnus_ui::theme::CygnusTheme;

/// True when the window is maximized or fullscreen (no resize edges,
/// the maximize button restores).
fn is_maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        let viewport = i.viewport();
        viewport.maximized.unwrap_or(false) || viewport.fullscreen.unwrap_or(false)
    })
}

/// Empty title-bar area filling `size`: drag moves the window,
/// double-click toggles maximize.
pub fn drag_area(ui: &mut egui::Ui, size: egui::Vec2) {
    let (_, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    if response.double_clicked() {
        let maximized = is_maximized(ui.ctx());
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    } else if response.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
}

/// Minimize / maximize / close, laid out right-to-left (close
/// outermost). `labels` = tooltips in that visual order.
pub fn window_controls(ui: &mut egui::Ui, theme: &CygnusTheme, labels: [&str; 3]) {
    let [minimize, maximize, close] = labels;
    if IconButton::new(Icon::Close)
        .tooltip(close)
        .show(ui, theme)
        .clicked()
    {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
    }
    if IconButton::new(Icon::Maximize)
        .tooltip(maximize)
        .show(ui, theme)
        .clicked()
    {
        let maximized = is_maximized(ui.ctx());
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    if IconButton::new(Icon::Minimize)
        .tooltip(minimize)
        .show(ui, theme)
        .clicked()
    {
        ui.ctx()
            .send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}

/// Resize direction under `pos` for a window of `rect` (`grip` wide
/// borders), `None` inside.
fn edge_at(rect: egui::Rect, pos: egui::Pos2, grip: f32) -> Option<egui::ResizeDirection> {
    use egui::ResizeDirection as D;
    let left = pos.x - rect.left() <= grip;
    let right = rect.right() - pos.x <= grip;
    let top = pos.y - rect.top() <= grip;
    let bottom = rect.bottom() - pos.y <= grip;
    match (left, right, top, bottom) {
        (true, _, true, _) => Some(D::NorthWest),
        (_, true, true, _) => Some(D::NorthEast),
        (true, _, _, true) => Some(D::SouthWest),
        (_, true, _, true) => Some(D::SouthEast),
        (true, ..) => Some(D::West),
        (_, true, ..) => Some(D::East),
        (_, _, true, _) => Some(D::North),
        (.., true) => Some(D::South),
        _ => None,
    }
}

/// Cursor shown over a resize edge.
fn edge_cursor(direction: egui::ResizeDirection) -> egui::CursorIcon {
    use egui::ResizeDirection as D;
    match direction {
        D::North => egui::CursorIcon::ResizeNorth,
        D::South => egui::CursorIcon::ResizeSouth,
        D::West => egui::CursorIcon::ResizeWest,
        D::East => egui::CursorIcon::ResizeEast,
        D::NorthWest => egui::CursorIcon::ResizeNorthWest,
        D::NorthEast => egui::CursorIcon::ResizeNorthEast,
        D::SouthWest => egui::CursorIcon::ResizeSouthWest,
        D::SouthEast => egui::CursorIcon::ResizeSouthEast,
    }
}

/// Edge resizing for the undecorated window: call once per frame
/// BEFORE the panels, so a press on the border starts a resize
/// instead of reaching the widget underneath.
pub fn resize_edges(ctx: &egui::Context, theme: &CygnusTheme) {
    if is_maximized(ctx) {
        return;
    }
    let grip = theme.spacing.xs;
    let rect = ctx.content_rect();
    let (hover, pressed) = ctx.input(|i| {
        (
            i.pointer.hover_pos(),
            i.pointer.button_pressed(egui::PointerButton::Primary),
        )
    });
    let Some(direction) = hover.and_then(|pos| edge_at(rect, pos, grip)) else {
        return;
    };
    ctx.set_cursor_icon(edge_cursor(direction));
    if pressed {
        ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edges_and_corners_resolve() {
        use egui::ResizeDirection as D;
        let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(100.0, 50.0));
        assert_eq!(edge_at(rect, egui::pos2(50.0, 25.0), 4.0), None);
        assert_eq!(edge_at(rect, egui::pos2(1.0, 25.0), 4.0), Some(D::West));
        assert_eq!(edge_at(rect, egui::pos2(99.0, 25.0), 4.0), Some(D::East));
        assert_eq!(edge_at(rect, egui::pos2(50.0, 1.0), 4.0), Some(D::North));
        assert_eq!(edge_at(rect, egui::pos2(50.0, 49.0), 4.0), Some(D::South));
        assert_eq!(edge_at(rect, egui::pos2(1.0, 1.0), 4.0), Some(D::NorthWest));
        assert_eq!(
            edge_at(rect, egui::pos2(99.0, 49.0), 4.0),
            Some(D::SouthEast)
        );
    }

    #[test]
    fn controls_render_idle() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let theme = CygnusTheme::dark();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.horizontal(|ui| {
                    window_controls(ui, &theme, ["min", "max", "close"]);
                    drag_area(ui, egui::vec2(40.0, 20.0));
                });
            });
            resize_edges(ui.ctx(), &theme);
        })
        .drop_without_applying_deltas();
    }
}

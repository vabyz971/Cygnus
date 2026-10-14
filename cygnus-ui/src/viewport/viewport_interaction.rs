// Cygnus — Suite créative professionnelle open source
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

//! Canvas mouse interactions: pan, wheel zoom and tools.
//!
//! [`zoom_factor_for_scroll`] is a pure function (wheel delta →
//! factor), testable without a UI context. [`handle_pointer`] applies
//! pan (middle button, or drag with the hand tool), zoom (wheel
//! anchored at the pointer, click with the magnifier tool) and reports
//! world positions for drawing tools.

use super::viewport_state::ViewportState;

/// The canvas's active tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ViewportTool {
    /// Move / select.
    #[default]
    Move,
    /// Hand: dragging moves the view.
    Pan,
    /// Magnifier: click = zoom in, right-click = zoom out.
    Zoom,
    /// Brush: dragging reports world positions.
    Brush,
}

impl ViewportTool {
    /// Cursor for the tool.
    pub fn cursor(self) -> egui::CursorIcon {
        match self {
            Self::Move => egui::CursorIcon::Move,
            Self::Pan => egui::CursorIcon::Grab,
            Self::Zoom => egui::CursorIcon::ZoomIn,
            Self::Brush => egui::CursorIcon::Crosshair,
        }
    }
}

/// Action reported by [`handle_pointer`], for the app to handle
/// (local state update, or engine send via channel).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ViewportAction {
    /// The view moved (pan).
    Panned,
    /// The zoom changed.
    Zoomed,
    /// Pointer's world position (click or drag with a drawing/move
    /// tool), in image pixels.
    PointerAtWorld(egui::Vec2),
}

/// Zoom factor for a vertical wheel delta.
///
/// Convention: wheel up (`delta_y > 0`) = zoom in (factor `> 1`),
/// wheel down = zoom out (`< 1`), no scroll = `1.0`. The per-event
/// factor is bounded to [1/4, 4] (trackpad guard); zoom bounds stay
/// enforced by [`ViewportState`].
pub fn zoom_factor_for_scroll(delta_y: f32) -> f32 {
    if delta_y == 0.0 {
        return 1.0;
    }
    (2.0f32.powf(delta_y * 0.002)).clamp(0.25, 4.0)
}

/// Applies pan/zoom/tools from pointer inputs.
///
/// Call after allocating the canvas, with its `response`.
/// Wheel zoom only counts when the canvas is hovered.
/// `viewport_center` is the widget's screen center (real geometry
/// `screen = center + offset + world * zoom`): wheel zoom and the
/// magnifier anchor there so the image point under the mouse stays
/// fixed (zoom at the cursor).
pub fn handle_pointer(
    ui: &mut egui::Ui,
    response: &egui::Response,
    state: &mut ViewportState,
    tool: ViewportTool,
    viewport_center: egui::Pos2,
) -> Vec<ViewportAction> {
    let mut actions = Vec::new();

    // Pan: middle button always, primary button with the hand tool.
    if response.dragged_by(egui::PointerButton::Middle)
        || (tool == ViewportTool::Pan && response.dragged_by(egui::PointerButton::Primary))
    {
        state.pan_by(response.drag_delta());
        actions.push(ViewportAction::Panned);
    }

    // Pointer-anchored wheel zoom (hovered canvas only).
    if response.hovered() {
        let delta_y = ui.input(|input| input.smooth_scroll_delta()).y;
        let factor = zoom_factor_for_scroll(delta_y);
        if factor != 1.0
            && let Some(anchor) = ui.ctx().pointer_latest_pos()
        {
            state.zoom_by_around(factor, anchor, viewport_center);
            actions.push(ViewportAction::Zoomed);
        }
    }

    // Magnifier click: in (left) / out (right), anchored at the pointer.
    if tool == ViewportTool::Zoom
        && let Some(anchor) = ui.ctx().pointer_latest_pos()
    {
        if response.clicked_by(egui::PointerButton::Primary) {
            state.zoom_by_around(1.25, anchor, viewport_center);
            actions.push(ViewportAction::Zoomed);
        } else if response.clicked_by(egui::PointerButton::Secondary) {
            state.zoom_by_around(0.8, anchor, viewport_center);
            actions.push(ViewportAction::Zoomed);
        }
    }

    // Drawing/move tools: report the world position.
    if (tool == ViewportTool::Brush || tool == ViewportTool::Move)
        && (response.dragged_by(egui::PointerButton::Primary)
            || response.clicked_by(egui::PointerButton::Primary))
        && let Some(pointer) = ui.ctx().pointer_latest_pos()
    {
        actions.push(ViewportAction::PointerAtWorld(
            state.screen_to_world(pointer),
        ));
    }

    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wheel_zoom_direction() {
        // Wheel up = zoom in.
        assert!(zoom_factor_for_scroll(50.0) > 1.0);
        // Wheel down = zoom out.
        assert!(zoom_factor_for_scroll(-50.0) < 1.0);
        // No scroll = no zoom.
        assert_eq!(zoom_factor_for_scroll(0.0), 1.0);
    }

    #[test]
    fn wheel_factor_bounded() {
        // Trackpad burst: bounded factor, never a brutal jump.
        assert_eq!(zoom_factor_for_scroll(10_000.0), 4.0);
        assert_eq!(zoom_factor_for_scroll(-10_000.0), 0.25);
    }

    #[test]
    fn tool_cursors_are_distinct() {
        assert_ne!(ViewportTool::Pan.cursor(), ViewportTool::Zoom.cursor());
        assert_ne!(ViewportTool::Brush.cursor(), ViewportTool::Move.cursor());
    }
}

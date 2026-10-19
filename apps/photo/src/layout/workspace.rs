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

//! Workspace orchestrator: fixed bars + docks + overlays.
//!
//! Required egui order: top/bottom panels first, central area
//! ([`Tree`](egui_tiles::Tree): one canvas panel per document +
//! tools, inspector, layers) next, modals on top. The
//! top (menus, modes) and bottom (status) bars stay fixed.
//! All logic lives in the regions and features.
use super::{bottom_bar, dock, overlays, top_bar, welcome};
use crate::app::PhotoApp;
use crate::commands::PhotoUiContext;

/// Global application composition (pure orchestration).
pub struct PhotoWorkspace;

impl PhotoWorkspace {
    /// Draws a full frame and queues the actions into `app.queue`.
    pub fn show(ui: &mut egui::Ui, app: &mut PhotoApp, ctx: &PhotoUiContext) {
        let mut actions = Vec::new();
        // 0. Undecorated window: border presses resize first.
        crate::ui::window_chrome::resize_edges(ui.ctx(), ctx.shared.theme());
        // 1-2. Top: title bar then persona / context bars (full width).
        actions.extend(top_bar::show_menu_bar(ui, app, ctx));
        actions.extend(top_bar::show_mode_bar(ui, app, ctx));
        // 3. Bottom status bar (reserved before the central area).
        bottom_bar::show(ui, app, ctx);
        // 4. Central area: welcome screen with no document, otherwise dockable
        // tiles (one canvas per document).
        egui::CentralPanel::default().show(ui, |ui| {
            if app.docs.is_empty() {
                actions.extend(welcome::show(ui, ctx));
            } else {
                actions.extend(dock::show_tree(ui, app, ctx));
            }
        });
        // 5. Modals and dialogs on top.
        actions.extend(overlays::show(ui, app, ctx));
        app.queue.extend(actions);
    }
}

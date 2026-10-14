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

//! "Tools" dock tab content: compact 32 px rail.
//!
//! Position-independent: placement (size, split, grid)
//! is managed by the `egui_tiles` tree (see `super::dock`). The tool choice bubbles up as
//! [`PhotoAction::SetTool`](crate::commands::PhotoAction); the
//! color is local UI state (direct mutation, no engine).

use crate::commands::PhotoAction;
use crate::state::OpenDocument;
use crate::ui::draw_tool_rail;
use cygnus_ui::theme::CygnusTheme;

/// Compact tool rail (tab content, no `Panel`).
pub fn draw_tools_content(
    ui: &mut egui::Ui,
    doc: &mut OpenDocument,
    theme: &CygnusTheme,
    texts: cygnus_i18n::PhotoCatalog,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let outcome = draw_tool_rail(
        ui,
        &mut doc.ui.tool,
        &mut doc.ui.brush.color,
        doc.ui.edit_mode,
        theme,
        texts,
    );
    if let Some(tool) = outcome.tool {
        actions.push(PhotoAction::SetTool(tool));
    }
    if let Some(shape) = outcome.new_shape {
        actions.push(PhotoAction::AddVectorLayer { shape });
    }
    actions
}

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

//! Top bars, full width:
//!
//! 1. title bar — brand, menus, window drag area and buttons (the OS
//!    decorations are off, see `ui::window_chrome`);
//! 2. persona row — Pixel / Vector / Layout tabs, quick actions on
//!    the right (history, view toggles, layout, export);
//! 3. context bar — tool switcher and the active tool's settings.
//!
//! Rows 1 and 2 share the chrome surface; rows 2 and 3 only exist
//! with an open document. Contents are delegated to the domain widgets
//! (`menubar`, `modebar`); here we only place the `egui::Panel`s and
//! convert the widget actions into
//! [`PhotoAction`](crate::commands::PhotoAction).

use crate::app::PhotoApp;
use crate::commands::{PhotoAction, PhotoUiContext};
use crate::ui::modebar::{QuickAction, QuickActionState, draw_persona_switch, draw_quick_actions};
use crate::ui::optionsbar::ToolOptions;
use crate::ui::window_chrome::{drag_area, window_controls};
use crate::ui::{
    MenuAvailability, PhotoCanvasTool, PhotoMenuAction, draw_menu_bar, draw_photo_modebar,
};
use cygnus_i18n::{PhotoCatalog, PhotoTextKey, TextKey};
use cygnus_ui::theme::CygnusTheme;

/// Product name shown at the left of the title bar.
const BRAND: &str = "Cygnus Photo";

/// Panel frame of a top row: `fill`, horizontal `md` margins,
/// vertical `margin_y`.
fn row_frame(theme: &CygnusTheme, fill: egui::Color32, margin_y: f32) -> egui::Frame {
    egui::Frame::NONE
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(
            theme.spacing.md as i8,
            margin_y as i8,
        ))
}

/// Converts a menu action into an app action.
fn menu_action_to_photo(action: PhotoMenuAction) -> PhotoAction {
    match action {
        PhotoMenuAction::NewDocument => PhotoAction::OpenNewDocumentDialog,
        PhotoMenuAction::OpenImage => PhotoAction::OpenImageDialog,
        PhotoMenuAction::OpenProject => PhotoAction::OpenProjectDialog,
        PhotoMenuAction::SaveProject => PhotoAction::SaveProject,
        PhotoMenuAction::SaveProjectAs => PhotoAction::SaveProjectAsDialog,
        PhotoMenuAction::Export => PhotoAction::OpenExportDialog,
        PhotoMenuAction::Quit => PhotoAction::Quit,
        PhotoMenuAction::Undo => PhotoAction::Undo,
        PhotoMenuAction::Redo => PhotoAction::Redo,
        PhotoMenuAction::AddEmptyLayer => PhotoAction::AddEmptyLayer,
        PhotoMenuAction::DuplicateLayer => PhotoAction::DuplicateSelectedLayer,
        PhotoMenuAction::FlipHorizontal => PhotoAction::FlipHorizontalSelected,
        PhotoMenuAction::FlipVertical => PhotoAction::FlipVerticalSelected,
        PhotoMenuAction::RotateClockwise => PhotoAction::RotateClockwiseSelected,
        PhotoMenuAction::RotateCounterclockwise => PhotoAction::RotateCounterclockwiseSelected,
        PhotoMenuAction::CropToDocument => PhotoAction::CropSelectedToDocument,
        PhotoMenuAction::AddMask => PhotoAction::AddMaskToSelected,
        PhotoMenuAction::DeleteLayer => PhotoAction::DeleteSelectedLayer,
        PhotoMenuAction::SelectAll => PhotoAction::SelectAll,
        PhotoMenuAction::Deselect => PhotoAction::Deselect,
        PhotoMenuAction::InvertSelection => PhotoAction::InvertSelection,
        PhotoMenuAction::MaskFromSelection => PhotoAction::MaskFromSelection,
        PhotoMenuAction::ToggleGrid => PhotoAction::ToggleGrid,
        PhotoMenuAction::TogglePreviewClip => PhotoAction::TogglePreviewClip,
        PhotoMenuAction::ZoomIn => PhotoAction::ZoomIn,
        PhotoMenuAction::ZoomOut => PhotoAction::ZoomOut,
        PhotoMenuAction::ZoomReset => PhotoAction::ZoomReset,
        PhotoMenuAction::CloseDocument => PhotoAction::CloseTab,
        PhotoMenuAction::ShowDockTab(tab) => PhotoAction::ShowDockTab(tab),
        PhotoMenuAction::ResetDockLayout => PhotoAction::ResetDockLayout,
        PhotoMenuAction::ShowHelp => PhotoAction::ShowHelp,
    }
}

/// Row 1: title bar (brand, menus, drag area, window buttons).
pub fn show_menu_bar(
    ui: &mut egui::Ui,
    app: &mut PhotoApp,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let theme = ctx.shared.theme();
    let has_document = app.active_doc_opt().is_some();
    egui::Panel::top("photo_menubar")
        .resizable(false)
        // Without a document the title bar ends the chrome.
        .show_separator_line(!has_document)
        .frame(row_frame(theme, theme.colors.bg_primary, theme.spacing.xxs))
        .show(ui, |ui| {
            let availability = MenuAvailability {
                has_document,
                can_undo: app.active_doc_opt().is_some_and(|doc| doc.ui.can_undo),
                can_redo: app.active_doc_opt().is_some_and(|doc| doc.ui.can_redo),
                has_selection: app
                    .active_doc_opt()
                    .is_some_and(|doc| doc.ui.selected.is_some()),
                preview_clip: app.active_doc_opt().is_some_and(|doc| doc.ui.preview_clip),
                has_marquee: app
                    .active_doc_opt()
                    .is_some_and(|doc| doc.ui.selection.is_some()),
            };
            let translator = ctx.shared.translator();
            let texts = PhotoCatalog::new(translator.language());
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(
                    egui::RichText::new(BRAND)
                        .size(theme.typography.body_size)
                        .strong()
                        .color(theme.colors.fg_strong),
                );
                ui.add_space(theme.spacing.md);
                actions.extend(
                    draw_menu_bar(ui, availability, theme, translator, texts)
                        .into_iter()
                        .map(menu_action_to_photo),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    window_controls(
                        ui,
                        theme,
                        [
                            texts.get(PhotoTextKey::WindowMinimize),
                            texts.get(PhotoTextKey::WindowMaximize),
                            translator.get(TextKey::Close),
                        ],
                    );
                    let size = egui::vec2(ui.available_width(), theme.sizes.button_md);
                    drag_area(ui, size);
                });
            });
        });
    actions
}

/// Converts a quick action into an app action.
fn quick_action_to_photo(action: QuickAction) -> PhotoAction {
    match action {
        QuickAction::Undo => PhotoAction::Undo,
        QuickAction::Redo => PhotoAction::Redo,
        QuickAction::ToggleGrid => PhotoAction::ToggleGrid,
        QuickAction::ToggleClip => PhotoAction::TogglePreviewClip,
        QuickAction::ResetLayout => PhotoAction::ResetDockLayout,
        QuickAction::Export => PhotoAction::OpenExportDialog,
    }
}

/// Rows 2 and 3: persona row (quick actions on the right) then the
/// context bar. Absent with no open document.
pub fn show_mode_bar(
    ui: &mut egui::Ui,
    app: &mut PhotoApp,
    ctx: &PhotoUiContext,
) -> Vec<PhotoAction> {
    let mut actions = Vec::new();
    let Some(doc) = app.active_doc_mut_opt() else {
        return actions;
    };
    let theme = ctx.shared.theme();
    let translator = ctx.shared.translator();
    let texts = PhotoCatalog::new(translator.language());
    egui::Panel::top("photo_personabar")
        .resizable(false)
        .frame(row_frame(theme, theme.colors.bg_primary, theme.spacing.sm))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                if let Some(mode) = draw_persona_switch(ui, &mut doc.ui.edit_mode, theme)
                    && !mode.supports(doc.ui.tool)
                {
                    // Tool missing from the new persona: fall back.
                    actions.push(PhotoAction::SetTool(PhotoCanvasTool::Move));
                }
                let state = QuickActionState {
                    can_undo: doc.ui.can_undo,
                    can_redo: doc.ui.can_redo,
                    show_grid: doc.ui.show_grid,
                    preview_clip: doc.ui.preview_clip,
                };
                let clicked = ui
                    .with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        draw_quick_actions(ui, state, theme, translator, texts)
                    })
                    .inner;
                actions.extend(clicked.map(quick_action_to_photo));
            });
        });
    egui::Panel::top("photo_contextbar")
        .resizable(false)
        .frame(row_frame(
            theme,
            theme.colors.bg_secondary,
            theme.spacing.xs,
        ))
        .show(ui, |ui| {
            ui.set_min_height(theme.sizes.button_md);
            let outcome = draw_photo_modebar(
                ui,
                doc.ui.edit_mode,
                doc.ui.tool,
                ToolOptions {
                    brush: &mut doc.ui.brush,
                    selection_kind: &mut doc.ui.selection_kind,
                    wand: &mut doc.ui.wand,
                },
                theme,
                texts,
            );
            if let Some(tool) = outcome.tool {
                actions.push(PhotoAction::SetTool(tool));
            }
        });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_menu_action_converts() {
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::Undo),
            PhotoAction::Undo
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::DeleteLayer),
            PhotoAction::DeleteSelectedLayer
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::FlipHorizontal),
            PhotoAction::FlipHorizontalSelected
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::RotateClockwise),
            PhotoAction::RotateClockwiseSelected
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::CropToDocument),
            PhotoAction::CropSelectedToDocument
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::NewDocument),
            PhotoAction::OpenNewDocumentDialog
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::OpenProject),
            PhotoAction::OpenProjectDialog
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::SaveProject),
            PhotoAction::SaveProject
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::SaveProjectAs),
            PhotoAction::SaveProjectAsDialog
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::ShowDockTab(
                crate::layout::dock::PhotoDockTab::Layers
            )),
            PhotoAction::ShowDockTab(crate::layout::dock::PhotoDockTab::Layers)
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::ResetDockLayout),
            PhotoAction::ResetDockLayout
        );
        assert_eq!(
            menu_action_to_photo(PhotoMenuAction::CloseDocument),
            PhotoAction::CloseTab
        );
    }
}

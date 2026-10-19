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

//! PHOTO context menu (top bar, domain widget).
//!
//! Six menus: File (new document with settings, open,
//! export with settings, quit), Edit (undo, redo),
//! Layer (empty, image, duplicate, mask, delete), View
//! (grid, zoom), Window (reopen a closed panel, reset
//! the dock layout), Help. Items via [`cygnus_ui::components::menu`].
//! The bar style uses square corners via `menu_bar_style`,
//! sub-menus keep the theme's rounded radius.
//! Generic labels come from the [`Catalog`](cygnus_i18n::Catalog) catalog,
//! Photo-specific strings from the [`PhotoCatalog`](cygnus_i18n::PhotoCatalog) catalog;
//! the settings windows (new document, export) are
//! app-owned modals (see `super::dialogs`).

use crate::layout::dock::PhotoDockTab;
use cygnus_i18n::{PhotoCatalog, PhotoTextKey};

use cygnus_i18n::{Catalog, TextKey};
use cygnus_ui::components::menu::{menu_bar_style, menu_item, menu_item_enabled, menu_style};
use cygnus_ui::theme::CygnusTheme;

/// Action reported by [`draw_menu_bar`], handled by the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhotoMenuAction {
    /// New document (opens the settings window).
    NewDocument,
    /// Open an image (non-blocking file picker).
    OpenImage,
    /// Open a `.cygp` project (new tab, non-blocking file picker).
    OpenProject,
    /// Save the project (known path) or fall back to
    /// "Save as".
    SaveProject,
    /// Save the project under a new `.cygp` path.
    SaveProjectAs,
    /// Export (opens the export settings window).
    Export,
    /// Quit the application.
    Quit,
    /// Undo / redo (worker).
    Undo,
    /// Redo.
    Redo,
    /// New empty layer.
    AddEmptyLayer,
    /// Duplicate the selected layer.
    DuplicateLayer,
    /// Horizontal mirror of the selected layer (O004).
    FlipHorizontal,
    /// Vertical mirror of the selected layer (O004).
    FlipVertical,
    /// 90° clockwise rotation (O004).
    RotateClockwise,
    /// 90° counterclockwise rotation (O004).
    RotateCounterclockwise,
    /// Crop the selected layer to the document (O004).
    CropToDocument,
    /// Add a mask to the selected layer.
    AddMask,
    /// Delete the selected layer.
    DeleteLayer,
    /// Toggle the canvas grid.
    ToggleGrid,
    /// Clip the preview to the document (else infinite plane: off-frame
    /// content stays visible around).
    TogglePreviewClip,
    /// Zoom in / out (anchored at center).
    ZoomIn,
    /// Zoom out.
    ZoomOut,
    /// Reset zoom to 100%.
    ZoomReset,
    /// Close the active document.
    CloseDocument,
    /// Reopen a closed dock tab.
    ShowDockTab(PhotoDockTab),
    /// Restore the default dock layout.
    ResetDockLayout,
    /// Open the help window.
    ShowHelp,
}

/// Availability flags for greying out entries (history, selection).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuAvailability {
    /// A document is open (otherwise everything else is locked).
    pub has_document: bool,
    /// Undo / redo available.
    pub can_undo: bool,
    /// Redo available.
    pub can_redo: bool,
    /// A layer is selected.
    pub has_selection: bool,
    /// The preview is clipped to the document (View menu check).
    pub preview_clip: bool,
}

/// Draws the menu bar and returns the actions.
///
/// Items are cygnus-ui ghost buttons (transparent background,
/// native egui hover highlight); clicking closes the open menu.
/// Generic labels come from the cygnus-ui catalog, Photo-specific
/// strings from the [`PhotoCatalog`] catalog.
pub fn draw_menu_bar(
    ui: &mut egui::Ui,
    availability: MenuAvailability,
    theme: &CygnusTheme,
    catalog: Catalog,
    texts: PhotoCatalog,
) -> Vec<PhotoMenuAction> {
    let mut actions = Vec::new();
    egui::Frame::NONE.show(ui, |ui| {
        // Apply the menu bar style (square corners)
        // via a Scope to avoid polluting the global style.
        let bar_style = {
            let s = ui.style_mut();
            menu_bar_style(theme).apply(s);
            // Subdued titles at rest, strong when hovered or open.
            s.visuals.widgets.inactive.fg_stroke.color = theme.colors.fg_secondary;
            s.clone()
        };
        ui.scope_builder(egui::UiBuilder::new().style(bar_style), |ui| {
            // Inner padding of menu titles (bigger clickable
            // areas, airy bar), all from the theme. Dropdown
            // items keep their fixed height (cygnus-ui min_size).
            ui.spacing_mut().button_padding = egui::vec2(theme.spacing.sm, theme.spacing.sm);
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button(catalog.get(TextKey::File), |ui| {
                    menu_style(theme).apply(ui.style_mut());
                    if menu_item(ui, theme, catalog.get(TextKey::NewDocument)) {
                        actions.push(PhotoMenuAction::NewDocument);
                    }
                    if menu_item(ui, theme, catalog.get(TextKey::Open)) {
                        actions.push(PhotoMenuAction::OpenImage);
                    }
                    if menu_item(ui, theme, texts.get(PhotoTextKey::OpenProject)) {
                        actions.push(PhotoMenuAction::OpenProject);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        catalog.get(TextKey::Save),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::SaveProject);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        texts.get(PhotoTextKey::SaveAs),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::SaveProjectAs);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        catalog.get(TextKey::Export),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::Export);
                    }
                    ui.separator();
                    if menu_item_enabled(
                        ui,
                        theme,
                        texts.get(PhotoTextKey::CloseDocument),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::CloseDocument);
                    }
                    if menu_item(ui, theme, catalog.get(TextKey::Quit)) {
                        actions.push(PhotoMenuAction::Quit);
                    }
                });
                ui.menu_button(catalog.get(TextKey::Edit), |ui| {
                    menu_style(theme).apply(ui.style_mut());
                    if menu_item_enabled(
                        ui,
                        theme,
                        catalog.get(TextKey::Undo),
                        availability.can_undo,
                    ) {
                        actions.push(PhotoMenuAction::Undo);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        catalog.get(TextKey::Redo),
                        availability.can_redo,
                    ) {
                        actions.push(PhotoMenuAction::Redo);
                    }
                });
                ui.menu_button(catalog.get(TextKey::Layer), |ui| {
                    menu_style(theme).apply(ui.style_mut());
                    if menu_item_enabled(
                        ui,
                        theme,
                        texts.get(PhotoTextKey::NewEmptyLayer),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::AddEmptyLayer);
                    }
                    if menu_item(ui, theme, texts.get(PhotoTextKey::LayerFromImage)) {
                        actions.push(PhotoMenuAction::OpenImage);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        texts.get(PhotoTextKey::DuplicateLayer),
                        availability.has_selection,
                    ) {
                        actions.push(PhotoMenuAction::DuplicateLayer);
                    }
                    // Transformations submenu (mirror, rotation,
                    // crop): same style, closing handled by items.
                    ui.menu_button(texts.get(PhotoTextKey::Transform), |ui| {
                        menu_style(theme).apply(ui.style_mut());
                        if menu_item_enabled(
                            ui,
                            theme,
                            texts.get(PhotoTextKey::FlipHorizontal),
                            availability.has_selection,
                        ) {
                            actions.push(PhotoMenuAction::FlipHorizontal);
                        }
                        if menu_item_enabled(
                            ui,
                            theme,
                            texts.get(PhotoTextKey::FlipVertical),
                            availability.has_selection,
                        ) {
                            actions.push(PhotoMenuAction::FlipVertical);
                        }
                        if menu_item_enabled(
                            ui,
                            theme,
                            texts.get(PhotoTextKey::RotateClockwise),
                            availability.has_selection,
                        ) {
                            actions.push(PhotoMenuAction::RotateClockwise);
                        }
                        if menu_item_enabled(
                            ui,
                            theme,
                            texts.get(PhotoTextKey::RotateCounterclockwise),
                            availability.has_selection,
                        ) {
                            actions.push(PhotoMenuAction::RotateCounterclockwise);
                        }
                        if menu_item_enabled(
                            ui,
                            theme,
                            texts.get(PhotoTextKey::CropToDocument),
                            availability.has_selection,
                        ) {
                            actions.push(PhotoMenuAction::CropToDocument);
                        }
                    });
                    if menu_item_enabled(
                        ui,
                        theme,
                        texts.get(PhotoTextKey::AddMask),
                        availability.has_selection,
                    ) {
                        actions.push(PhotoMenuAction::AddMask);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        texts.get(PhotoTextKey::DeleteLayer),
                        availability.has_selection,
                    ) {
                        actions.push(PhotoMenuAction::DeleteLayer);
                    }
                });
                ui.menu_button(catalog.get(TextKey::View), |ui| {
                    menu_style(theme).apply(ui.style_mut());
                    if menu_item_enabled(
                        ui,
                        theme,
                        catalog.get(TextKey::Grid),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::ToggleGrid);
                    }
                    // Check via the catalog (rendering identical to before).
                    let crop_label = format!(
                        "[{}] {}",
                        if availability.preview_clip { "x" } else { " " },
                        texts.get(PhotoTextKey::CropPreviewToDocument)
                    );
                    if menu_item_enabled(ui, theme, crop_label.as_str(), availability.has_document)
                    {
                        actions.push(PhotoMenuAction::TogglePreviewClip);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        catalog.get(TextKey::ZoomIn),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::ZoomIn);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        catalog.get(TextKey::ZoomOut),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::ZoomOut);
                    }
                    if menu_item_enabled(
                        ui,
                        theme,
                        texts.get(PhotoTextKey::Zoom100),
                        availability.has_document,
                    ) {
                        actions.push(PhotoMenuAction::ZoomReset);
                    }
                });
                ui.menu_button(catalog.get(TextKey::Window), |ui| {
                    menu_style(theme).apply(ui.style_mut());
                    // Pinned canvases (non-closable): only
                    // tools, inspector, layers, and history
                    // can be reopened.
                    for tab in [
                        PhotoDockTab::Tools,
                        PhotoDockTab::Inspector,
                        PhotoDockTab::Layers,
                        PhotoDockTab::History,
                        PhotoDockTab::Histogram,
                    ] {
                        if menu_item(ui, theme, catalog.get(tab.title_key())) {
                            actions.push(PhotoMenuAction::ShowDockTab(tab));
                        }
                    }
                    ui.separator();
                    if menu_item(ui, theme, texts.get(PhotoTextKey::ResetLayout)) {
                        actions.push(PhotoMenuAction::ResetDockLayout);
                    }
                });
                ui.menu_button(catalog.get(TextKey::Help), |ui| {
                    menu_style(theme).apply(ui.style_mut());
                    if menu_item(ui, theme, texts.get(PhotoTextKey::About)) {
                        actions.push(PhotoMenuAction::ShowHelp);
                    }
                });
            });
        });
    });
    actions
}

#[cfg(test)]
mod tests {
    use super::*;
    use cygnus_i18n::Language;

    #[test]
    fn menu_bar_renders_without_panic_and_idle() {
        let ctx = egui::Context::default();
        cygnus_ui::theme::setup_fonts(&ctx);
        let theme = CygnusTheme::dark();
        for language in [Language::En, Language::Fr] {
            let catalog = Catalog::new(language);
            let texts = PhotoCatalog::new(language);
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let actions =
                        draw_menu_bar(ui, MenuAvailability::default(), &theme, catalog, texts);
                    assert!(actions.is_empty(), "aucun clic sans interaction");
                });
            })
            .drop_without_applying_deltas();
        }
    }
}

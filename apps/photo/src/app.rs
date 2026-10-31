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

//! Main screen of Cygnus Photo.

use cygnus_ui_kit::dock::DockContainer;
use cygnus_ui_kit::i18n::{Key, tr};
use cygnus_ui_kit::icons;
use cygnus_ui_kit::theme::{metrics, palette};
use cygnus_ui_kit::widgets::{self, DocumentTab, Persona, Tool};
use egui::{Align, Frame, Layout, Margin, Panel, RichText, Ui};

use crate::keys;
use crate::panels::{self, PanelViewer};
use crate::placeholder;

/// Tools of the beta (spec, section 3). Text, pen and shape tools of the
/// mockup are left out.
const TOOLS: [(&str, Key); 11] = [
    (icons::ARROWS_OUT_CARDINAL, keys::TOOL_MOVE),
    (icons::CROP, keys::TOOL_CROP),
    (icons::SELECTION, keys::TOOL_RECT_SELECT),
    (icons::LASSO, keys::TOOL_LASSO),
    (icons::MAGIC_WAND, keys::TOOL_MAGIC_WAND),
    (icons::PAINT_BRUSH, keys::TOOL_BRUSH),
    (icons::STAMP, keys::TOOL_CLONE),
    (icons::ERASER, keys::TOOL_ERASER),
    (icons::EYEDROPPER, keys::TOOL_EYEDROPPER),
    (icons::HAND, keys::TOOL_PAN),
    (icons::MAGNIFYING_GLASS, keys::TOOL_ZOOM),
];

/// Index of the brush in [`TOOLS`], the tool shown in the mockup.
const BRUSH: usize = 5;

/// Personas: icon, label, available in the beta.
const PERSONAS: [(&str, Key, bool); 5] = [
    (icons::APERTURE, keys::PERSONA_PHOTO, true),
    (icons::SUN, keys::PERSONA_DEVELOP, false),
    (icons::WAVES, keys::PERSONA_LIQUIFY, false),
    (icons::STACK, keys::PERSONA_TONE_MAPPING, false),
    (icons::PAPER_PLANE_TILT, keys::PERSONA_EXPORT, true),
];

/// Inert menu entries, per menu. They come alive with their features.
const MENUS: [(Key, &[Key]); 8] = [
    (
        keys::MENU_FILE,
        &[
            keys::FILE_NEW,
            keys::FILE_OPEN,
            keys::FILE_SAVE,
            keys::FILE_EXPORT,
        ],
    ),
    (
        keys::MENU_EDIT,
        &[keys::EDIT_UNDO, keys::EDIT_REDO, keys::EDIT_PREFERENCES],
    ),
    (
        keys::MENU_DOCUMENT,
        &[keys::DOCUMENT_SETTINGS, keys::DOCUMENT_IMAGE_SIZE],
    ),
    (
        keys::MENU_LAYER,
        &[
            keys::LAYER_NEW,
            keys::LAYER_DUPLICATE,
            keys::LAYER_DELETE,
            keys::LAYER_MERGE_DOWN,
        ],
    ),
    (
        keys::MENU_SELECT,
        &[keys::SELECT_ALL, keys::SELECT_NONE, keys::SELECT_INVERT],
    ),
    (
        keys::MENU_FILTERS,
        &[keys::FILTERS_GAUSSIAN_BLUR, keys::FILTERS_UNSHARP_MASK],
    ),
    (
        keys::MENU_VIEW,
        &[
            keys::VIEW_ZOOM_IN,
            keys::VIEW_ZOOM_OUT,
            keys::VIEW_FIT,
            keys::VIEW_ACTUAL_SIZE,
        ],
    ),
    (keys::MENU_HELP, &[keys::HELP_ABOUT]),
];

/// The Cygnus Photo application window.
pub struct PhotoApp {
    dock: DockContainer<panels::Panel>,
    persona: usize,
    tool: usize,
    document: usize,
    placeholder: placeholder::State,
}

impl PhotoApp {
    /// Creates the main screen with the saved workspace, or the default one.
    pub fn new() -> Self {
        Self {
            dock: DockContainer::new(panels::load_layout(), panels::default_layout),
            persona: 0,
            tool: BRUSH,
            document: 0,
            placeholder: placeholder::State::default(),
        }
    }

    fn menus(&mut self, ui: &mut Ui) {
        for (menu, entries) in MENUS {
            // The Window menu goes before Help, as in the mockup.
            if menu == keys::MENU_HELP {
                ui.menu_button(tr(keys::MENU_WINDOW), |ui| self.window_menu(ui));
            }
            ui.menu_button(tr(menu), |ui| {
                for entry in entries {
                    ui.add_enabled(false, egui::Button::new(tr(*entry)));
                }
                if menu == keys::MENU_FILE {
                    ui.separator();
                    if ui.button(tr(keys::FILE_QUIT)).clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            });
        }
    }

    fn window_menu(&mut self, ui: &mut Ui) {
        for panel in panels::Panel::ALL {
            let mut open = self.dock.is_open(&panel);
            if ui.checkbox(&mut open, tr(panel.title())).changed() {
                self.dock.set_open(&panel, open);
            }
        }
        ui.separator();
        if ui.button(tr(keys::WINDOW_RESET_LAYOUT)).clicked() {
            self.dock.reset();
            ui.close();
        }
    }

    fn persona_bar(&mut self, ui: &mut Ui) {
        let personas = PERSONAS.map(|(icon, label, enabled)| Persona {
            icon,
            label: tr(label),
            enabled,
            disabled_hint: tr(keys::PERSONA_UNAVAILABLE),
        });
        widgets::persona_bar(ui, icons::DOTS_NINE, &personas, &mut self.persona, |ui| {
            let export = egui::Button::new((
                widgets::icon(icons::EXPORT, metrics::ICON - 2.0).color(palette::TEXT_MUTED),
                RichText::new(tr(keys::ACTION_EXPORT)).color(palette::TEXT_MUTED),
            ))
            .min_size(egui::vec2(0.0, 34.0))
            .fill(palette::CHROME)
            .stroke(egui::Stroke::new(1.0, palette::BORDER));
            ui.add_enabled(false, export);
            ui.add_space(metrics::SPACING);
            widgets::vertical_separator(ui, 24.0);
            ui.add_space(metrics::SPACING);
            for (icon, hint) in [
                (icons::ARROW_CLOCKWISE, keys::EDIT_REDO),
                (icons::ARROW_COUNTER_CLOCKWISE, keys::EDIT_UNDO),
            ] {
                widgets::icon_button(ui, icon, tr(hint));
            }
        });
    }

    fn tool_rail(&mut self, ui: &mut Ui) {
        let tools = TOOLS.map(|(icon, name)| Tool {
            icon,
            name: tr(name),
        });
        widgets::tool_rail(ui, &tools, &mut self.tool, placeholder::color_swatches);
    }

    fn canvas(&mut self, ui: &mut Ui) {
        let tabs = [DocumentTab {
            title: placeholder::document_title(),
            close_hint: tr(keys::DOCUMENT_CLOSE),
        }];
        widgets::document_tabs(ui, &tabs, &mut self.document, placeholder::document_info);

        Panel::bottom("canvas-zoom")
            .exact_size(metrics::STATUS_BAR_HEIGHT)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(palette::INSET)
                    .inner_margin(Margin::symmetric(metrics::PADDING as i8, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    widgets::icon_button(ui, icons::MINUS, tr(keys::ZOOM_OUT));
                    ui.label(
                        RichText::new(placeholder::ZOOM_PERCENT.to_owned() + " %").monospace(),
                    );
                    widgets::icon_button(ui, icons::PLUS, tr(keys::ZOOM_IN));
                    ui.add_enabled(false, egui::Button::new(tr(keys::ZOOM_FIT)).frame(false));
                });
            });
        Panel::bottom("canvas-exif")
            .exact_size(30.0)
            .resizable(false)
            .frame(
                Frame::new()
                    .fill(palette::CANVAS)
                    .inner_margin(Margin::symmetric(metrics::PADDING as i8 * 4, 0)),
            )
            .show(ui, |ui| {
                ui.horizontal_centered(placeholder::exif_line);
            });

        egui::CentralPanel::default()
            .frame(Frame::new().fill(palette::CANVAS))
            .show(ui, |ui| {
                ui.centered_and_justified(|ui| {
                    ui.label(RichText::new(tr(keys::CANVAS_PENDING)).color(palette::TEXT_MUTED));
                });
            });
    }

    fn status_bar(ui: &mut Ui) {
        widgets::status_bar(ui, |ui| {
            ui.label(widgets::icon(icons::CURSOR, metrics::ICON - 4.0).color(palette::TEXT_MUTED));
            ui.label(RichText::new(placeholder::status_action()).color(palette::TEXT));
            ui.label(RichText::new(tr(keys::STATUS_SHORTCUTS)).color(palette::TEXT_MUTED));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                ui.label(
                    widgets::icon(icons::CHECK, metrics::ICON - 4.0).color(palette::TEXT_MUTED),
                );
                ui.label(RichText::new(tr(keys::STATUS_GPU)).color(palette::TEXT_MUTED));
                ui.add_space(metrics::SPACING);
                ui.label(RichText::new(tr(keys::STATUS_SAVED)).color(palette::TEXT));
                ui.label(
                    widgets::icon(icons::CHECK_CIRCLE, metrics::ICON - 4.0).color(palette::SUCCESS),
                );
            });
        });
    }
}

impl eframe::App for PhotoApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        widgets::menu_bar(ui, tr(keys::APP_NAME), |ui| self.menus(ui));
        self.persona_bar(ui);
        widgets::options_bar(ui, |ui| {
            placeholder::brush_options(ui, &mut self.placeholder)
        });
        Self::status_bar(ui);
        self.tool_rail(ui);
        let mut viewer = PanelViewer {
            placeholder: &mut self.placeholder,
        };
        self.dock.show(ui, &mut viewer);
        self.canvas(ui);
    }

    fn on_exit(&mut self) {
        panels::save_layout(self.dock.layout());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        palette::CANVAS.to_normalized_gamma_f32()
    }
}

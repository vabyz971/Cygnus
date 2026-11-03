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

use cygnus_commands::Source;
use cygnus_photo_core::{Depth, Document};
use cygnus_ui_kit::dock::DockContainer;
use cygnus_ui_kit::i18n::{Key, tr, tr_format};
use cygnus_ui_kit::icons;
use cygnus_ui_kit::theme::{metrics, palette};
use cygnus_ui_kit::widgets::{self, DocumentTab, Persona, Tool};
use egui::{Align, Frame, Layout, Margin, Panel, RichText, Ui};

use crate::bench::BenchRun;
use crate::canvas::{self, Session};
use crate::demo;
use crate::keys;
use crate::layers;
use crate::panels::{self, PanelViewer};
use crate::tools::{self, ToolSettings};
use crate::view::ViewState;

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
/// Index of the pan tool in [`TOOLS`].
const PAN: usize = 9;

/// Personas: icon, label, available in the beta.
const PERSONAS: [(&str, Key, bool); 5] = [
    (icons::APERTURE, keys::PERSONA_PHOTO, true),
    (icons::SUN, keys::PERSONA_DEVELOP, false),
    (icons::WAVES, keys::PERSONA_LIQUIFY, false),
    (icons::STACK, keys::PERSONA_TONE_MAPPING, false),
    (icons::PAPER_PLANE_TILT, keys::PERSONA_EXPORT, true),
];

/// Help text of each tool of [`TOOLS`], shown in the status bar.
const TOOL_HELP: [Key; 11] = [
    keys::TOOL_HELP_MOVE,
    keys::TOOL_HELP_CROP,
    keys::TOOL_HELP_RECT_SELECT,
    keys::TOOL_HELP_LASSO,
    keys::TOOL_HELP_MAGIC_WAND,
    keys::TOOL_HELP_BRUSH,
    keys::TOOL_HELP_CLONE,
    keys::TOOL_HELP_ERASER,
    keys::TOOL_HELP_EYEDROPPER,
    keys::TOOL_HELP_PAN,
    keys::TOOL_HELP_ZOOM,
];

/// Inert menu entries, per menu. They come alive with their features; the
/// Edit and Layer menus are built by the application.
const MENUS: [(Key, &[Key]); 7] = [
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
    (keys::MENU_HELP, &[keys::HELP_ABOUT]),
];

/// Color format and size of the document, for the document tab bar.
fn document_info(ui: &mut Ui, document: &Document) {
    widgets::icon_button(ui, icons::INFO, tr(keys::DOCUMENT_INFO));
    let mono = |text: &str| RichText::new(text).monospace().color(palette::TEXT_MUTED);
    ui.label(mono(tr(match document.depth {
        Depth::U8 => keys::DOCUMENT_FORMAT_8,
        Depth::U16 => keys::DOCUMENT_FORMAT_16,
    })));
    ui.label(mono(&tr_format(
        keys::DOCUMENT_DIMENSIONS,
        &[
            ("width", &document.width.to_string()),
            ("height", &document.height.to_string()),
        ],
    )));
}

/// The Cygnus Photo application window.
pub struct PhotoApp {
    dock: DockContainer<panels::Panel>,
    persona: usize,
    tool: usize,
    document: usize,
    session: Session,
    view: ViewState,
    tools: ToolSettings,
    bench: Option<BenchRun>,
    /// Development: frames drawn, and where to write a screenshot of the
    /// window (`CYGNUS_SCREENSHOT`) before closing.
    frames: u32,
    screenshot: Option<std::path::PathBuf>,
}

/// Development: writes a screenshot of the window to this PNG file after a
/// few frames, then closes (reviews of the interface, docs/tasks).
pub const SCREENSHOT_ENV: &str = "CYGNUS_SCREENSHOT";
/// Frames drawn before the screenshot, so that the canvas and the
/// histogram are up to date.
const SCREENSHOT_FRAME: u32 = 40;

impl PhotoApp {
    /// Creates the main screen with the saved workspace, or the default one.
    pub fn new() -> Self {
        Self {
            dock: DockContainer::new(panels::load_layout(), panels::default_layout),
            persona: 0,
            tool: BRUSH,
            document: 0,
            session: Session::from_commands(
                demo::commands([
                    tr(keys::DEMO_LAYER_SKY),
                    tr(keys::DEMO_LAYER_SUN),
                    tr(keys::DEMO_LAYER_MOUNTAINS),
                    tr(keys::DEMO_LAYER_LAKE),
                    tr(keys::DEMO_LAYER_MIST),
                ]),
                Source::Test,
            ),
            view: ViewState::default(),
            tools: ToolSettings::default(),
            bench: None,
            frames: 0,
            screenshot: std::env::var_os(SCREENSHOT_ENV).map(Into::into),
        }
    }

    /// Creates the main screen running the fluidity benchmark.
    pub fn new_bench() -> Result<Self, String> {
        let (run, session) = BenchRun::start()?;
        Ok(Self {
            session,
            bench: Some(run),
            ..Self::new()
        })
    }

    fn menus(&mut self, ui: &mut Ui, render_state: Option<&eframe::egui_wgpu::RenderState>) {
        for (menu, entries) in MENUS {
            if menu == keys::MENU_EDIT {
                ui.menu_button(tr(menu), |ui| self.edit_menu(ui));
                continue;
            }
            if menu == keys::MENU_LAYER {
                ui.menu_button(tr(menu), |ui| self.layer_menu(ui, render_state));
                continue;
            }
            // View and Window go before Help, as in the mockup.
            if menu == keys::MENU_HELP {
                ui.menu_button(tr(keys::MENU_VIEW), |ui| self.view_menu(ui));
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

    fn edit_menu(&mut self, ui: &mut Ui) {
        let (undo, redo) = (self.session.bus().can_undo(), self.session.bus().can_redo());
        if ui
            .add_enabled(
                undo,
                egui::Button::new(tr(keys::EDIT_UNDO)).shortcut_text("Ctrl+Z"),
            )
            .clicked()
        {
            self.session.undo();
            ui.close();
        }
        if ui
            .add_enabled(
                redo,
                egui::Button::new(tr(keys::EDIT_REDO)).shortcut_text("Ctrl+Maj+Z"),
            )
            .clicked()
        {
            self.session.redo();
            ui.close();
        }
        ui.add_enabled(false, egui::Button::new(tr(keys::EDIT_PREFERENCES)));
    }

    fn layer_menu(&mut self, ui: &mut Ui, render_state: Option<&eframe::egui_wgpu::RenderState>) {
        if ui.button(tr(keys::LAYER_NEW)).clicked() {
            let number = self.session.document().nodes.len().to_string();
            let name = tr_format(keys::LAYER_DEFAULT_NAME, &[("number", &number)]);
            let (parent, index) = match self
                .view
                .selected
                .and_then(|id| self.session.document().node(id))
            {
                Some(node) => (
                    node.parent,
                    node.parent.and_then(|p| {
                        self.session
                            .document()
                            .children(p)
                            .iter()
                            .position(|c| *c == node.id)
                            .map(|i| i + 1)
                    }),
                ),
                None => (None, None),
            };
            self.session
                .user(cygnus_photo_core::PhotoCommand::LayerCreate(
                    cygnus_photo_core::commands::LayerCreate {
                        name,
                        parent,
                        index,
                        kind: Default::default(),
                    },
                ));
            ui.close();
        }
        let Some(id) = self.view.selected else {
            return;
        };
        ui.separator();
        let mut picked = None;
        layers::menu_entries(
            ui,
            self.session.document(),
            &mut self.view,
            id,
            &mut |command| {
                picked = Some(command);
            },
        );
        if let Some(command) = picked {
            layers::run(&mut self.session, render_state, command);
        }
    }

    /// Ctrl+Z and Ctrl+Shift+Z (or Ctrl+Y).
    fn shortcuts(&mut self, ui: &Ui) {
        let (undo, redo) = ui.input_mut(|input| {
            let redo_shift = egui::KeyboardShortcut::new(
                egui::Modifiers::COMMAND | egui::Modifiers::SHIFT,
                egui::Key::Z,
            );
            let redo_y = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
            let undo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
            let redo = input.consume_shortcut(&redo_shift) || input.consume_shortcut(&redo_y);
            (input.consume_shortcut(&undo), redo)
        });
        if undo {
            self.session.undo();
        }
        if redo {
            self.session.redo();
        }
    }

    /// Development screenshot (`CYGNUS_SCREENSHOT`).
    fn screenshot(&mut self, ui: &Ui) {
        let Some(path) = self.screenshot.clone() else {
            return;
        };
        self.frames += 1;
        ui.ctx().request_repaint();
        if self.frames == SCREENSHOT_FRAME {
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(Default::default()));
        }
        let image = ui.input(|input| {
            input.events.iter().find_map(|event| match event {
                egui::Event::Screenshot { image, .. } => Some(image.clone()),
                _ => None,
            })
        });
        if let Some(image) = image {
            let data = image
                .pixels
                .iter()
                .flat_map(|c| c.to_srgba_unmultiplied().map(u16::from))
                .collect();
            let image = cygnus_photo_render::Image {
                width: image.size[0] as u32,
                height: image.size[1] as u32,
                bits: cygnus_photo_render::ExportBits::Eight,
                data,
            };
            let mut bytes = Vec::new();
            let written = image
                .write_png(&mut bytes)
                .map_err(|error| error.to_string())
                .and_then(|()| std::fs::write(&path, bytes).map_err(|error| error.to_string()));
            if let Err(error) = written {
                eprintln!("cygnus-photo: screenshot {}: {error}", path.display());
            }
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn view_menu(&mut self, ui: &mut Ui) {
        if ui.button(tr(keys::VIEW_ZOOM_IN)).clicked() {
            self.session.zoom_step(1);
        }
        if ui.button(tr(keys::VIEW_ZOOM_OUT)).clicked() {
            self.session.zoom_step(-1);
        }
        if ui.button(tr(keys::VIEW_FIT)).clicked() {
            self.session.fit();
        }
        if ui.button(tr(keys::VIEW_ACTUAL_SIZE)).clicked() {
            self.session.actual_size();
        }
    }

    fn zoom_text(&self) -> String {
        let value = format!("{:.1}", self.session.zoom() * 100.0)
            .replace('.', tr(keys::NUMBER_DECIMAL_SEPARATOR));
        tr_format(keys::UNIT_PERCENT, &[("value", &value)])
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
            if widgets::icon_button(ui, icons::ARROW_CLOCKWISE, tr(keys::EDIT_REDO)).clicked() {
                self.session.redo();
            }
            if widgets::icon_button(ui, icons::ARROW_COUNTER_CLOCKWISE, tr(keys::EDIT_UNDO))
                .clicked()
            {
                self.session.undo();
            }
        });
    }

    fn tool_rail(&mut self, ui: &mut Ui) {
        let tools = TOOLS.map(|(icon, name)| Tool {
            icon,
            name: tr(name),
        });
        let settings = &mut self.tools;
        widgets::tool_rail(ui, &tools, &mut self.tool, |ui| {
            tools::color_swatches(ui, settings);
        });
    }

    fn canvas(
        &mut self,
        ui: &mut Ui,
        canvas_error: Option<String>,
        render_state: Option<&eframe::egui_wgpu::RenderState>,
    ) {
        let tabs = [DocumentTab {
            title: tr(keys::DEMO_TITLE),
            close_hint: tr(keys::DOCUMENT_CLOSE),
        }];
        let document = self.session.document();
        widgets::document_tabs(ui, &tabs, &mut self.document, |ui| {
            document_info(ui, document);
        });

        let zoom_text = self.zoom_text();
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
                    if widgets::icon_button(ui, icons::MINUS, tr(keys::ZOOM_OUT)).clicked() {
                        self.session.zoom_step(-1);
                    }
                    ui.label(RichText::new(zoom_text).monospace());
                    if widgets::icon_button(ui, icons::PLUS, tr(keys::ZOOM_IN)).clicked() {
                        self.session.zoom_step(1);
                    }
                    if ui
                        .add(egui::Button::new(tr(keys::ZOOM_FIT)).frame(false))
                        .clicked()
                    {
                        self.session.fit();
                    }
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
                // Camera settings (AFF-3) come with the metadata of J6;
                // until then only the mask target is shown.
                let target = self
                    .view
                    .selected
                    .filter(|_| self.view.mask)
                    .and_then(|id| self.session.document().node(id))
                    .map(|node| node.name.clone());
                ui.horizontal_centered(|ui| {
                    if let Some(name) = target {
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(tr_format(keys::MASK_TARGET, &[("name", &name)]))
                                    .color(palette::TEXT_MUTED),
                            );
                            ui.label(
                                widgets::icon(icons::PAINT_BRUSH, metrics::ICON - 4.0)
                                    .color(palette::TEXT_MUTED),
                            );
                        });
                    }
                });
            });

        let pan_tool = self.tool == PAN;
        egui::CentralPanel::default()
            .frame(Frame::new().fill(palette::CANVAS))
            .show(ui, |ui| match canvas_error {
                None => {
                    if let Some(run) = &mut self.bench {
                        let size = ui.max_rect().size() * ui.ctx().pixels_per_point();
                        if run.frame(&mut self.session, [size.x, size.y], render_state) {
                            ui.ctx().request_repaint();
                        } else {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                        }
                    }
                    self.session.show(ui, pan_tool);
                }
                Some(error) => {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            RichText::new(tr_format(keys::CANVAS_ERROR, &[("error", &error)]))
                                .color(palette::TEXT_MUTED),
                        );
                    });
                }
            });
    }

    fn status_bar(&self, ui: &mut Ui, render_state: Option<&eframe::egui_wgpu::RenderState>) {
        let coarse = render_state.and_then(canvas::coarse);
        let preview = self.session.in_gesture();
        widgets::status_bar(ui, |ui| {
            ui.label(widgets::icon(icons::CURSOR, metrics::ICON - 4.0).color(palette::TEXT_MUTED));
            match self.session.message() {
                Some(error) => {
                    ui.label(
                        RichText::new(tr_format(keys::STATUS_REFUSED, &[("error", error)]))
                            .color(palette::WARNING),
                    );
                }
                None => {
                    ui.label(RichText::new(tr(TOOL_HELP[self.tool])).color(palette::TEXT));
                    ui.label(RichText::new(tr(keys::STATUS_SHORTCUTS)).color(palette::TEXT_MUTED));
                }
            }
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
                // Coarse fallback (0025): a preview during a drag, or a
                // cache too small for the visible tiles.
                let coarse_text = if preview {
                    Some(keys::STATUS_COARSE_COMPUTING)
                } else {
                    coarse.map(|_| keys::STATUS_COARSE_MEMORY)
                };
                if let Some(key) = coarse_text {
                    ui.add_space(metrics::SPACING);
                    ui.label(RichText::new(tr(key)).color(palette::TEXT_MUTED));
                }
            });
        });
    }
}

impl eframe::App for PhotoApp {
    fn ui(&mut self, ui: &mut Ui, frame: &mut eframe::Frame) {
        let render_state = frame.wgpu_render_state().cloned();
        let canvas_error = render_state.as_ref().and_then(canvas::last_error);
        self.shortcuts(ui);
        self.screenshot(ui);
        widgets::menu_bar(ui, tr(keys::APP_NAME), |ui| {
            self.menus(ui, render_state.as_ref())
        });
        self.persona_bar(ui);
        widgets::options_bar(ui, |ui| tools::brush_options(ui, &mut self.tools));
        self.status_bar(ui, render_state.as_ref());
        self.tool_rail(ui);
        let mut viewer = PanelViewer {
            session: &mut self.session,
            view: &mut self.view,
            render_state: render_state.as_ref(),
        };
        self.dock.show(ui, &mut viewer);
        self.canvas(ui, canvas_error, render_state.as_ref());
    }

    fn on_exit(&mut self) {
        panels::save_layout(self.dock.layout());
    }

    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        palette::CANVAS.to_normalized_gamma_f32()
    }
}

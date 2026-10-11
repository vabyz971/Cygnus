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
// MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Document creation dialog: tabs, visual preset grid
//! (clickable cards) and the selected preset's settings.

use crate::i18n::{PhotoCatalog, PhotoTextKey};
use ui_kit::components::Tabs;
use ui_kit::components::{Button, ButtonVariant};
use ui_kit::dialogs::{CygnusModal, ModalAction};
use ui_kit::theme::CygnusTheme;
use ui_kit::theme::typography::{body_size, body_text, heading_text};

/// Print resolution for physical units (in, cm, pica).
pub const DOC_DPI: f64 = 300.0;
/// Maximum document dimension (memory guard).
pub const DOC_MAX_DIMENSION: f64 = 16_000.0;

/// Dimension units (pica = 1/6 inch, typographic unit).
pub const DOC_UNITS: &[&str] = &["px", "in", "cm", "pica"];

/// New-document preset (dimensions in pixels).
struct Preset {
    /// Displayed-name key on the card.
    name: PhotoTextKey,
    /// Width in pixels.
    px_w: f64,
    /// Height in pixels.
    px_h: f64,
    /// Suggested unit index (2 = cm for print, 0 = px otherwise).
    unit: usize,
}

/// Print-mode presets (shown in centimeters).
const IMPRESSION_PRESETS: &[Preset] = &[
    Preset {
        name: PhotoTextKey::PresetA4Portrait,
        px_w: 2480.0,
        px_h: 3508.0,
        unit: 2,
    },
    Preset {
        name: PhotoTextKey::PresetA4Landscape,
        px_w: 3508.0,
        px_h: 2480.0,
        unit: 2,
    },
    Preset {
        name: PhotoTextKey::PresetLetterPortrait,
        px_w: 2550.0,
        px_h: 3300.0,
        unit: 2,
    },
    Preset {
        name: PhotoTextKey::PresetLetterLandscape,
        px_w: 3300.0,
        px_h: 2550.0,
        unit: 2,
    },
];

/// Digital-mode presets (shown in pixels).
const NUMERIQUE_PRESETS: &[Preset] = &[
    Preset {
        name: PhotoTextKey::Preset4kUhd,
        px_w: 3840.0,
        px_h: 2160.0,
        unit: 0,
    },
    Preset {
        name: PhotoTextKey::Preset8kUhd,
        px_w: 7680.0,
        px_h: 4320.0,
        unit: 0,
    },
    Preset {
        name: PhotoTextKey::Preset16by9FullHd,
        px_w: 1920.0,
        px_h: 1080.0,
        unit: 0,
    },
    Preset {
        name: PhotoTextKey::PresetSquare,
        px_w: 1080.0,
        px_h: 1080.0,
        unit: 0,
    },
];

/// New-document orientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocOrientation {
    /// Height >= width.
    #[default]
    Portrait,
    /// Width >= height.
    Paysage,
}

/// Validated choice from the "Create document" window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewDocumentChoice {
    /// Create a blank document (dimensions in pixels).
    Create(u32, u32),
    /// Open an image from disk.
    OpenImage,
}

/// Pixels per unit (physical units at [`DOC_DPI`] DPI).
#[must_use]
pub fn px_per_unit(unit: usize) -> f64 {
    match DOC_UNITS.get(unit).copied().unwrap_or("px") {
        "in" => DOC_DPI,
        "cm" => DOC_DPI / 2.54,
        "pica" => DOC_DPI / 6.0,
        _ => 1.0,
    }
}

/// Converts a value entered in `unit` to (bounded) pixels.
#[must_use]
pub fn to_px(value: f64, unit: usize) -> u32 {
    (value * px_per_unit(unit))
        .round()
        .clamp(1.0, DOC_MAX_DIMENSION) as u32
}

/// Converts pixels to the display unit.
#[must_use]
pub fn from_px(px: f64, unit: usize) -> f64 {
    px / px_per_unit(unit).max(1.0)
}

/// "Create document" window state.
#[derive(Debug, Clone)]
pub struct CreateDocumentDialogState {
    /// Window visible.
    pub open: bool,
    /// Active tab (0 = Print, 1 = Digital).
    pub tab: usize,
    /// Selected preset (`None` = custom dimensions).
    pub preset: Option<usize>,
    /// Index into [`DOC_UNITS`].
    pub unit: usize,
    /// Width in the current unit.
    pub width: f64,
    /// Height in the current unit.
    pub height: f64,
    /// Orientation (switch = swap when needed).
    pub orientation: DocOrientation,
}

impl Default for CreateDocumentDialogState {
    fn default() -> Self {
        Self {
            open: false,
            tab: 0,
            preset: Some(0),
            // Consistent with the Print tab (A4 in cm).
            unit: 2,
            width: from_px(2480.0, 2),
            height: from_px(3508.0, 2),
            orientation: DocOrientation::Portrait,
        }
    }
}

impl CreateDocumentDialogState {
    /// Opens the window on the Print tab's first format.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Final dimensions in pixels (bounded, at least 1x1).
    #[must_use]
    pub fn px_dimensions(&self) -> (u32, u32) {
        (to_px(self.width, self.unit), to_px(self.height, self.unit))
    }

    /// Applies a preset: the preset's suggested unit + dimensions in
    /// pixels converted to that unit, inferred orientation.
    pub fn apply_preset(&mut self, preset: usize, px_w: f64, px_h: f64, unit: usize) {
        self.preset = Some(preset);
        self.unit = unit;
        self.width = from_px(px_w, unit);
        self.height = from_px(px_h, unit);
        self.orientation = if px_w >= px_h {
            DocOrientation::Paysage
        } else {
            DocOrientation::Portrait
        };
    }

    /// Changes the unit while keeping the pixel size (re-converted
    /// entry, no dimension jump).
    pub fn set_unit(&mut self, unit: usize) {
        let unit = unit.min(DOC_UNITS.len() - 1);
        if unit == self.unit {
            return;
        }
        let (px_w, px_h) = self.px_dimensions();
        self.unit = unit;
        self.width = from_px(f64::from(px_w), unit);
        self.height = from_px(f64::from(px_h), unit);
    }

    /// Manual entry: custom dimensions (no preset
    /// selected anymore).
    pub fn set_custom_size(&mut self, width: f64, height: f64) {
        self.width = width;
        self.height = height;
        self.preset = None;
    }

    /// Switches orientation (swaps dimensions when needed).
    /// The format becomes custom (no preset matches).
    pub fn set_orientation(&mut self, orientation: DocOrientation) {
        self.orientation = orientation;
        match orientation {
            DocOrientation::Portrait => {
                if self.width > self.height {
                    std::mem::swap(&mut self.width, &mut self.height);
                }
            }
            DocOrientation::Paysage => {
                if self.height > self.width {
                    std::mem::swap(&mut self.width, &mut self.height);
                }
            }
        }
        self.preset = None;
    }
}

/// Preset of the active tab (0 = Print, else Digital).
fn active_presets(tab: usize) -> &'static [Preset] {
    if tab == 0 {
        IMPRESSION_PRESETS
    } else {
        NUMERIQUE_PRESETS
    }
}

/// Draws a preset card: rounded white square (canvas ratio) with an
/// accented border when selected, name below.
/// Returns `true` when the card was clicked.
#[must_use]
fn draw_preset_card(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    preset: &Preset,
    selected: bool,
    texts: PhotoCatalog,
) -> bool {
    let size = egui::vec2(theme.sizes.preset_card, theme.sizes.preset_card);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    // Rounded white square whose proportions reflect the canvas ratio.
    let aspect = (preset.px_w / preset.px_h) as f32;
    let (cw, ch) = if aspect >= 1.0 {
        (size.x * 0.76, size.x * 0.76 / aspect)
    } else {
        (size.x * 0.76 * aspect, size.x * 0.76)
    };
    let card = egui::Rect::from_center_size(rect.center(), egui::vec2(cw, ch));

    let painter = ui.painter();
    painter.rect_filled(card, theme.radius.md, theme.colors.fg_primary);
    let stroke_width = if selected {
        theme.borders.selected
    } else {
        theme.borders.thin
    };
    let stroke_color = if selected {
        theme.colors.accent
    } else if resp.hovered() {
        theme.colors.item_hover
    } else {
        theme.colors.border
    };
    painter.rect_stroke(
        card,
        theme.radius.md,
        egui::Stroke::new(stroke_width, stroke_color),
        egui::StrokeKind::Inside,
    );

    // Preset name centered under the square.
    let pos = egui::pos2(rect.center().x, card.bottom() + theme.spacing.xs);
    let name_color = if selected {
        theme.colors.accent
    } else {
        theme.colors.fg_secondary
    };
    painter.text(
        pos,
        egui::Align2::CENTER_TOP,
        texts.get(preset.name),
        egui::FontId::proportional(body_size(theme)),
        name_color,
    );

    // Tooltip with the real dimensions in pixels.
    resp.clone().on_hover_text(format!(
        "{} — {} × {} px",
        texts.get(preset.name),
        preset.px_w,
        preset.px_h
    ));
    resp.clicked()
}

/// 2×2 preset grid of the current tab. Returns the clicked index.
fn draw_preset_grid(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    tab: usize,
    preset: &mut Option<usize>,
    texts: PhotoCatalog,
) -> Option<usize> {
    let presets = active_presets(tab);
    let mut clicked = None;
    egui::Grid::new(("create_document_grid", tab))
        .num_columns(2)
        .spacing(egui::vec2(theme.spacing.sm, theme.spacing.sm))
        .show(ui, |ui| {
            for (index, item) in presets.iter().enumerate() {
                if draw_preset_card(ui, theme, item, *preset == Some(index), texts) {
                    clicked = Some(index);
                }
                if (index + 1) % 2 == 0 {
                    ui.end_row();
                }
            }
        });
    clicked
}

/// Draws the "Create document" window: full-width
/// tabs, then two vertical columns (preset grid on the
/// left, preset settings on the right).
/// Returns the validated choice (create or open image).
pub fn draw_create_document_dialog(
    ui_ctx: &crate::commands::PhotoUiContext,
    state: &mut CreateDocumentDialogState,
) -> Option<NewDocumentChoice> {
    if !state.open {
        return None;
    }
    let ctx = ui_ctx.shared.ctx();
    let theme = *ui_ctx.shared.theme();
    let translator = ui_ctx.shared.translator();
    let texts = crate::i18n::PhotoCatalog::new(translator.language());
    let mut confirm: Option<NewDocumentChoice> = None;
    let mut open = true;
    let mut open_image = false;

    let action = CygnusModal::new(
        texts.get(crate::i18n::PhotoTextKey::CreateDocumentTitle),
        translator.get(ui_kit::i18n::TextKey::Create),
        translator.get(ui_kit::i18n::TextKey::Cancel),
    )
    .size(egui::vec2(470.0, 440.0))
    .show(ctx, &mut open, |ui| {
        // Full-width tabs (above the columns).
        let mut tab = state.tab;
        let tabs = [
            texts.get(crate::i18n::PhotoTextKey::PrintTab),
            texts.get(crate::i18n::PhotoTextKey::DigitalTab),
        ];
        Tabs::new(&tabs).show(ui, &theme, &mut tab);
        state.tab = tab;
        // The stored index belongs to the other tab: invalid
        // out of range (both lists have 4 entries today,
        // robust if that changes).
        if state.preset.is_some_and(|i| i >= active_presets(tab).len()) {
            state.preset = None;
        }
        ui.add_space(theme.spacing.sm);

        // Two left-aligned vertical columns.
        ui.horizontal_top(|ui| {
            // Column 1: 2×2 preset grid + image opening.
            ui.vertical(|ui| {
                if let Some(index) = draw_preset_grid(ui, &theme, tab, &mut state.preset, texts) {
                    let preset = &active_presets(tab)[index];
                    state.apply_preset(index, preset.px_w, preset.px_h, preset.unit);
                }
                ui.add_space(theme.spacing.sm);
                if Button::new(texts.get(crate::i18n::PhotoTextKey::OpenImageFile))
                    .variant(ButtonVariant::Secondary)
                    .show(ui, &theme)
                    .clicked()
                {
                    open_image = true;
                }
            });

            ui.add_space(theme.spacing.lg);

            // Column 2: selected preset settings (vertical).
            ui.vertical(|ui| {
                let presets = active_presets(state.tab);

                ui.label(heading_text(
                    &theme,
                    translator.get(ui_kit::i18n::TextKey::Settings),
                ));
                ui.add_space(theme.spacing.xs);
                if let Some(selected) = state.preset.and_then(|index| presets.get(index)) {
                    ui.label(body_text(&theme, texts.get(selected.name)).strong());
                }
                ui.label(body_text(
                    &theme,
                    &format!(
                        "{} × {} px",
                        to_px(state.width, state.unit),
                        to_px(state.height, state.unit)
                    ),
                ));
                ui.separator();

                // Unit change = re-conversion (pixel
                // size preserved).
                let mut unit = state.unit;
                ui_kit::components::Select::new(
                    texts.get(crate::i18n::PhotoTextKey::UnitLabel),
                    DOC_UNITS,
                )
                .stacked()
                .show(ui, &theme, &mut unit);
                if unit != state.unit {
                    state.set_unit(unit);
                }

                // Manual entry = custom format.
                let max = from_px(DOC_MAX_DIMENSION, state.unit);
                let mut width = state.width;
                ui_kit::components::NumberInput::new(
                    texts.get(crate::i18n::PhotoTextKey::WidthLabel),
                )
                .range(0.01..=max)
                .stacked()
                .show(ui, &theme, &mut width);
                let mut height = state.height;
                ui_kit::components::NumberInput::new(
                    texts.get(crate::i18n::PhotoTextKey::HeightLabel),
                )
                .range(0.01..=max)
                .stacked()
                .show(ui, &theme, &mut height);
                if width != state.width || height != state.height {
                    state.set_custom_size(width, height);
                }

                ui.horizontal(|ui| {
                    ui.label(body_text(
                        &theme,
                        texts.get(crate::i18n::PhotoTextKey::OrientationLabel),
                    ));
                    let portrait = state.orientation == DocOrientation::Portrait;
                    if Button::new(texts.get(crate::i18n::PhotoTextKey::PortraitLabel))
                        .variant(if portrait {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        })
                        .show(ui, &theme)
                        .clicked()
                    {
                        state.set_orientation(DocOrientation::Portrait);
                    }
                    let paysage = state.orientation == DocOrientation::Paysage;
                    if Button::new(texts.get(crate::i18n::PhotoTextKey::LandscapeLabel))
                        .variant(if paysage {
                            ButtonVariant::Primary
                        } else {
                            ButtonVariant::Secondary
                        })
                        .show(ui, &theme)
                        .clicked()
                    {
                        state.set_orientation(DocOrientation::Paysage);
                    }
                });
            });
        });
    });
    state.open = open && !open_image;
    if open_image {
        confirm = Some(NewDocumentChoice::OpenImage);
    } else if action == Some(ModalAction::Confirm) {
        let (width, height) = state.px_dimensions();
        confirm = Some(NewDocumentChoice::Create(width, height));
    }
    confirm
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic fullscreen entry for dialog tests.
    fn test_input(screen: egui::Rect, time: f64) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            ..Default::default()
        }
    }

    /// Regression: the "Create" button is painted (inside the window
    /// frame) and a synthetic click on it confirms the dialog
    /// (returns the dimensions and closes).
    #[test]
    fn confirm_button_painted_and_clickable() {
        use egui::{Event, Modifiers, PointerButton, Pos2};
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let mut state = CreateDocumentDialogState {
            open: true,
            ..Default::default()
        };
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0));

        // Frame 1: layout, locate the painted "Create" text.
        // (Two passes: the first paints transparently because
        // of the window appear fade.)
        for t in [0.0, 1.0] {
            ctx.run_ui(test_input(screen, t), |ui| {
                let c = ui.ctx().clone();
                let _ = draw_create_document_dialog(
                    &crate::commands::PhotoUiContext::for_frame(&c),
                    &mut state,
                );
            })
            .drop_without_applying_deltas();
        }
        let out = ctx.run_ui(test_input(screen, 1.0), |ui| {
            let c = ui.ctx().clone();
            let _ = draw_create_document_dialog(
                &crate::commands::PhotoUiContext::for_frame(&c),
                &mut state,
            );
        });
        let mut creer_pos = None;
        fn find_text(shape: &egui::Shape, wanted: &str, out: &mut Option<egui::Pos2>) {
            match shape {
                egui::Shape::Text(ts) if ts.galley.text() == wanted => *out = Some(ts.pos),
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        find_text(s, wanted, out);
                    }
                }
                _ => {}
            }
        }
        for shape in &out.shapes {
            find_text(&shape.shape, "Créer", &mut creer_pos);
        }
        out.drop_without_applying_deltas();
        let pos = creer_pos.expect("bouton Créer peint (atteignable)");

        // Synthetic click (press + release) at the label center.
        let click = Pos2::new(pos.x + 12.0, pos.y + 7.0);
        let mut retour = None;
        for (t, pressed) in [(1.1, true), (1.2, false)] {
            let mut raw = test_input(screen, t);
            raw.events.push(Event::PointerButton {
                pos: click,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            ctx.run_ui(raw, |ui| {
                let c = ui.ctx().clone();
                retour = draw_create_document_dialog(
                    &crate::commands::PhotoUiContext::for_frame(&c),
                    &mut state,
                );
            })
            .drop_without_applying_deltas();
        }
        // Default: A4 Portrait in cm → 2480 × 3508 px.
        assert_eq!(retour, Some(NewDocumentChoice::Create(2480, 3508)));
        assert!(!state.open, "la validation referme le dialogue");
    }

    /// Regression: the "Open an image…" button is painted and a
    /// click on it returns the open choice (and closes).
    #[test]
    fn open_image_button_returns_open_choice() {
        use egui::{Event, Modifiers, PointerButton, Pos2};
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let mut state = CreateDocumentDialogState {
            open: true,
            ..Default::default()
        };
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0));

        for t in [0.0, 1.0] {
            ctx.run_ui(test_input(screen, t), |ui| {
                let c = ui.ctx().clone();
                let _ = draw_create_document_dialog(
                    &crate::commands::PhotoUiContext::for_frame(&c),
                    &mut state,
                );
            })
            .drop_without_applying_deltas();
        }
        let out = ctx.run_ui(test_input(screen, 1.0), |ui| {
            let c = ui.ctx().clone();
            let _ = draw_create_document_dialog(
                &crate::commands::PhotoUiContext::for_frame(&c),
                &mut state,
            );
        });
        let mut image_pos = None;
        fn find_text(shape: &egui::Shape, wanted: &str, out: &mut Option<egui::Pos2>) {
            match shape {
                egui::Shape::Text(ts) if ts.galley.text() == wanted => *out = Some(ts.pos),
                egui::Shape::Vec(shapes) => {
                    for s in shapes {
                        find_text(s, wanted, out);
                    }
                }
                _ => {}
            }
        }
        for shape in &out.shapes {
            find_text(&shape.shape, "Ouvrir une image…", &mut image_pos);
        }
        out.drop_without_applying_deltas();
        let pos = image_pos.expect("bouton Ouvrir une image peint");

        let click = Pos2::new(pos.x + 12.0, pos.y + 7.0);
        let mut retour = None;
        for (t, pressed) in [(1.1, true), (1.2, false)] {
            let mut raw = test_input(screen, t);
            raw.events.push(Event::PointerButton {
                pos: click,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            });
            ctx.run_ui(raw, |ui| {
                let c = ui.ctx().clone();
                retour = draw_create_document_dialog(
                    &crate::commands::PhotoUiContext::for_frame(&c),
                    &mut state,
                );
            })
            .drop_without_applying_deltas();
        }
        assert_eq!(retour, Some(NewDocumentChoice::OpenImage));
        assert!(!state.open, "le choix referme le dialogue");
    }

    /// Regression: the window fits on screen (compact content,
    /// no horizontal overflow).
    #[test]
    fn dialog_window_fits_screen() {
        use egui::Pos2;
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let mut state = CreateDocumentDialogState {
            open: true,
            ..Default::default()
        };
        let screen = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(1280.0, 800.0));
        ctx.run_ui(test_input(screen, 0.0), |ui| {
            let c = ui.ctx().clone();
            let _ = draw_create_document_dialog(
                &crate::commands::PhotoUiContext::for_frame(&c),
                &mut state,
            );
        })
        .drop_without_applying_deltas();
        let win = ctx
            .memory(|m| {
                m.layer_id_at(screen.center())
                    .and_then(|l| m.area_rect(l.id))
            })
            .expect("fenetre modale posee");
        assert!(
            win.max.x <= screen.max.x && win.min.x >= 0.0,
            "fenêtre contenue en largeur : {win:?}"
        );
        assert!(
            win.max.y <= screen.max.y && win.min.y >= 0.0,
            "fenêtre contenue en hauteur : {win:?}"
        );
    }

    #[test]
    fn dialog_renders_and_applies_numerique_preset() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        let mut state = CreateDocumentDialogState {
            open: true,
            tab: 1,
            ..Default::default()
        };
        let presets = active_presets(1);
        state.apply_preset(2, presets[2].px_w, presets[2].px_h, presets[2].unit);
        assert_eq!(state.px_dimensions(), (1920, 1080));
        assert_eq!(state.orientation, DocOrientation::Paysage);
        assert_eq!(state.unit, 0);
        ctx.run_ui(egui::RawInput::default(), |ui| {
            let ctx = ui.ctx().clone();
            let _ = draw_create_document_dialog(
                &crate::commands::PhotoUiContext::for_frame(&ctx),
                &mut state,
            );
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn impression_preset_switches_unit_to_cm() {
        let mut state = CreateDocumentDialogState::default();
        let preset = &active_presets(0)[0]; // A4 Portrait
        state.apply_preset(0, preset.px_w, preset.px_h, preset.unit);
        assert_eq!(state.unit, 2);
        // 2480 px at 300 DPI = ~21 cm (within epsilon).
        assert!((state.width - 21.0).abs() < 0.1);
        assert_eq!(state.orientation, DocOrientation::Portrait);
        assert_eq!(state.px_dimensions(), (2480, 3508));
    }

    #[test]
    fn preset_cards_reflect_canvas_ratio() {
        let imp = active_presets(0);
        assert_eq!(imp[0].name, PhotoTextKey::PresetA4Portrait);
        assert!(imp[0].px_h > imp[0].px_w);
        let num = active_presets(1);
        assert_eq!(num[3].name, PhotoTextKey::PresetSquare);
        assert_eq!(num[3].px_w, num[3].px_h);
    }

    #[test]
    fn default_state_matches_impression_cm() {
        let state = CreateDocumentDialogState::default();
        assert_eq!(state.preset, Some(0));
        assert_eq!(state.unit, 2);
        assert_eq!(state.px_dimensions(), (2480, 3508));
    }

    #[test]
    fn set_unit_preserves_pixel_size() {
        let mut state = CreateDocumentDialogState::default();
        // A4 ≈ 21 cm → switch to px: same pixels.
        state.set_unit(0);
        assert_eq!(state.unit, 0);
        assert_eq!(state.px_dimensions(), (2480, 3508));
        assert!((state.width - 2480.0).abs() < 0.01);
        // Back to cm: stable.
        state.set_unit(2);
        assert_eq!(state.px_dimensions(), (2480, 3508));
    }

    #[test]
    fn custom_size_and_orientation_clear_preset() {
        let mut state = CreateDocumentDialogState::default();
        assert_eq!(state.preset, Some(0));
        state.set_custom_size(100.0, 50.0);
        assert_eq!(state.preset, None);
        state.apply_preset(2, 1920.0, 1080.0, 0);
        assert_eq!(state.preset, Some(2));
        state.set_orientation(DocOrientation::Portrait);
        assert_eq!(state.preset, None);
        // Dimensions swapped for portrait.
        assert!(state.height >= state.width);
    }
}

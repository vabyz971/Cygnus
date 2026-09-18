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
// MERCHANTABILITY OR FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! Dialogue de création de document : onglets, grille de présets
//! visuels (cartes cliquables) et paramètres du préset sélectionné.

use ui_kit::components::Tabs;
use ui_kit::components::{Button, ButtonVariant};
use ui_kit::dialogs::{CygnusModal, ModalAction};
use ui_kit::theme::CygnusTheme;
use ui_kit::theme::typography::{body_size, body_text, heading_text};

/// Résolution d'impression pour les unités physiques (in, cm, pica).
pub const DOC_DPI: f64 = 300.0;
/// Dimension maximale d'un document (garde-fou mémoire).
pub const DOC_MAX_DIMENSION: f64 = 16_000.0;

/// Unités de dimension (pica = 1/6 de pouce, unité typographique).
pub const DOC_UNITS: &[&str] = &["px", "in", "cm", "pica"];

/// Largeur d'une carte de préset (carré, sa hauteur est identique).
const PRESET_CARD_SIZE: f32 = 90.0;

/// Préset de nouveau document (dimensions en pixels).
struct Preset {
    /// Nom affiché sur la carte.
    name: &'static str,
    /// Largeur en pixels.
    px_w: f64,
    /// Hauteur en pixels.
    px_h: f64,
    /// Index d'unité suggéré (2 = cm pour l'impression, 0 = px sinon).
    unit: usize,
}

/// Présets du mode Impression (affichés en centimètres).
const IMPRESSION_PRESETS: &[Preset] = &[
    Preset {
        name: "A4 Portrait",
        px_w: 2480.0,
        px_h: 3508.0,
        unit: 2,
    },
    Preset {
        name: "A4 Paysage",
        px_w: 3508.0,
        px_h: 2480.0,
        unit: 2,
    },
    Preset {
        name: "Lettre Portrait",
        px_w: 2550.0,
        px_h: 3300.0,
        unit: 2,
    },
    Preset {
        name: "Lettre Paysage",
        px_w: 3300.0,
        px_h: 2550.0,
        unit: 2,
    },
];

/// Présets du mode Numérique (affichés en pixels).
const NUMERIQUE_PRESETS: &[Preset] = &[
    Preset {
        name: "4K UHD",
        px_w: 3840.0,
        px_h: 2160.0,
        unit: 0,
    },
    Preset {
        name: "8K UHD",
        px_w: 7680.0,
        px_h: 4320.0,
        unit: 0,
    },
    Preset {
        name: "16:9 Full HD",
        px_w: 1920.0,
        px_h: 1080.0,
        unit: 0,
    },
    Preset {
        name: "Carré",
        px_w: 1080.0,
        px_h: 1080.0,
        unit: 0,
    },
];

/// Orientation d'un nouveau document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DocOrientation {
    /// Hauteur >= largeur.
    #[default]
    Portrait,
    /// Largeur >= hauteur.
    Paysage,
}

/// Choix validé depuis la fenêtre « Créer un document ».
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewDocumentChoice {
    /// Créer un document vierge (dimensions en pixels).
    Create(u32, u32),
    /// Ouvrir une image depuis le disque.
    OpenImage,
}

/// Nombre de pixels par unité (unités physiques à [`DOC_DPI`] DPI).
#[must_use]
pub fn px_per_unit(unit: usize) -> f64 {
    match DOC_UNITS.get(unit).copied().unwrap_or("px") {
        "in" => DOC_DPI,
        "cm" => DOC_DPI / 2.54,
        "pica" => DOC_DPI / 6.0,
        _ => 1.0,
    }
}

/// Convertit une valeur saisie dans `unit` vers des pixels (bornés).
#[must_use]
pub fn to_px(value: f64, unit: usize) -> u32 {
    (value * px_per_unit(unit))
        .round()
        .clamp(1.0, DOC_MAX_DIMENSION) as u32
}

/// Convertit des pixels vers l'unité d'affichage.
#[must_use]
pub fn from_px(px: f64, unit: usize) -> f64 {
    px / px_per_unit(unit).max(1.0)
}

/// État de la fenêtre « Créer un document ».
#[derive(Debug, Clone)]
pub struct CreateDocumentDialogState {
    /// Fenêtre visible.
    pub open: bool,
    /// Onglet actif (0 = Impression, 1 = Numérique).
    pub tab: usize,
    /// Préset sélectionné (`None` = dimensions personnalisées).
    pub preset: Option<usize>,
    /// Index dans [`DOC_UNITS`].
    pub unit: usize,
    /// Largeur dans l'unité courante.
    pub width: f64,
    /// Hauteur dans l'unité courante.
    pub height: f64,
    /// Orientation (bascule = échange si besoin).
    pub orientation: DocOrientation,
}

impl Default for CreateDocumentDialogState {
    fn default() -> Self {
        Self {
            open: false,
            tab: 0,
            preset: Some(0),
            // Cohérent avec l'onglet Impression (A4 en cm).
            unit: 2,
            width: from_px(2480.0, 2),
            height: from_px(3508.0, 2),
            orientation: DocOrientation::Portrait,
        }
    }
}

impl CreateDocumentDialogState {
    /// Ouvre la fenêtre sur le premier format de l'onglet Impression.
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Dimensions finales en pixels (bornées, au moins 1x1).
    #[must_use]
    pub fn px_dimensions(&self) -> (u32, u32) {
        (to_px(self.width, self.unit), to_px(self.height, self.unit))
    }

    /// Applique un préset : unité suggérée du préset + dimensions en
    /// pixels converties dans cette unité, orientation déduite.
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

    /// Change l'unité en conservant la taille pixels (saisie
    /// reconvertie, pas de saut de dimensions).
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

    /// Saisie manuelle : dimensions personnalisées (plus aucun préset
    /// sélectionné).
    pub fn set_custom_size(&mut self, width: f64, height: f64) {
        self.width = width;
        self.height = height;
        self.preset = None;
    }

    /// Bascule l'orientation (échange les dimensions si besoin).
    /// Le format devient personnalisé (aucun préset ne correspond).
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

/// Préset de l'onglet actif (0 = Impression, sinon Numérique).
fn active_presets(tab: usize) -> &'static [Preset] {
    if tab == 0 {
        IMPRESSION_PRESETS
    } else {
        NUMERIQUE_PRESETS
    }
}

/// Dessine une carte de préset : carré blanc arrondi (ratio du
/// canvas) avec bordure accentuée si sélectionnée, nom dessous.
/// Retourne `true` si la carte a été cliquée.
#[must_use]
fn draw_preset_card(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    preset: &Preset,
    selected: bool,
) -> bool {
    let size = egui::vec2(PRESET_CARD_SIZE, PRESET_CARD_SIZE);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());

    // Carré blanc arrondi dont les proportions reflètent le ratio du canvas.
    let aspect = (preset.px_w / preset.px_h) as f32;
    let (cw, ch) = if aspect >= 1.0 {
        (size.x * 0.76, size.x * 0.76 / aspect)
    } else {
        (size.x * 0.76 * aspect, size.x * 0.76)
    };
    let card = egui::Rect::from_center_size(rect.center(), egui::vec2(cw, ch));

    let painter = ui.painter();
    painter.rect_filled(card, theme.radius.md, theme.colors.fg_primary);
    let stroke_width = if selected { 2.5 } else { 1.0 };
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

    // Nom du préset centré sous le carré.
    let pos = egui::pos2(rect.center().x, card.bottom() + theme.spacing.xs);
    let name_color = if selected {
        theme.colors.accent
    } else {
        theme.colors.fg_secondary
    };
    painter.text(
        pos,
        egui::Align2::CENTER_TOP,
        preset.name,
        egui::FontId::proportional(body_size(theme)),
        name_color,
    );

    // Infobulle avec les dimensions réelles en pixels.
    resp.clone().on_hover_text(format!(
        "{} — {} × {} px",
        preset.name, preset.px_w, preset.px_h
    ));
    resp.clicked()
}

/// Grille 2×2 de présets de l'onglet courant. Retourne l'index cliqué.
fn draw_preset_grid(
    ui: &mut egui::Ui,
    theme: &CygnusTheme,
    tab: usize,
    preset: &mut Option<usize>,
) -> Option<usize> {
    let presets = active_presets(tab);
    let mut clicked = None;
    egui::Grid::new(("create_document_grid", tab))
        .num_columns(2)
        .spacing(egui::vec2(theme.spacing.sm, theme.spacing.sm))
        .show(ui, |ui| {
            for (index, item) in presets.iter().enumerate() {
                if draw_preset_card(ui, theme, item, *preset == Some(index)) {
                    clicked = Some(index);
                }
                if (index + 1) % 2 == 0 {
                    ui.end_row();
                }
            }
        });
    clicked
}

/// Dessine la fenêtre « Créer un document » : onglets pleine
/// largeur, puis deux colonnes verticales (grille de présets à
/// gauche, paramètres du préset à droite).
/// Retourne le choix validé (création ou ouverture d'image).
pub fn draw_create_document_dialog(
    ctx: &egui::Context,
    state: &mut CreateDocumentDialogState,
) -> Option<NewDocumentChoice> {
    if !state.open {
        return None;
    }
    let theme = CygnusTheme::dark();
    let mut confirm: Option<NewDocumentChoice> = None;
    let mut open = true;
    let mut open_image = false;

    let action = CygnusModal::new("Créer un document", "Créer", "Annuler")
        .size(egui::vec2(470.0, 440.0))
        .show(ctx, &mut open, |ui| {
            // Onglets pleine largeur (au-dessus des colonnes).
            let mut tab = state.tab;
            Tabs::new(&["Impression", "Numérique"]).show(ui, &theme, &mut tab);
            state.tab = tab;
            // L'index mémorisé appartient à l'autre onglet : invalide
            // hors plage (les deux listes ont 4 entrées aujourd'hui,
            // robuste si ça change).
            if state.preset.is_some_and(|i| i >= active_presets(tab).len()) {
                state.preset = None;
            }
            ui.add_space(theme.spacing.sm);

            // Deux colonnes verticales alignées à gauche.
            ui.horizontal_top(|ui| {
                // Colonne 1 : grille 2×2 de présets + ouverture d'image.
                ui.vertical(|ui| {
                    if let Some(index) = draw_preset_grid(ui, &theme, tab, &mut state.preset) {
                        let preset = &active_presets(tab)[index];
                        state.apply_preset(index, preset.px_w, preset.px_h, preset.unit);
                    }
                    ui.add_space(theme.spacing.sm);
                    if Button::new("Ouvrir une image…")
                        .variant(ButtonVariant::Secondary)
                        .show(ui, &theme)
                        .clicked()
                    {
                        open_image = true;
                    }
                });

                ui.add_space(theme.spacing.lg);

                // Colonne 2 : paramètres du préset sélectionné (vertical).
                ui.vertical(|ui| {
                    let presets = active_presets(state.tab);

                    ui.label(heading_text(&theme, "Paramètres"));
                    ui.add_space(theme.spacing.xs);
                    if let Some(selected) = state.preset.and_then(|index| presets.get(index)) {
                        ui.label(body_text(&theme, selected.name).strong());
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

                    // Changement d'unité = reconversion (taille
                    // pixels préservée).
                    let mut unit = state.unit;
                    ui_kit::components::Select::new("Unité", DOC_UNITS)
                        .stacked()
                        .show(ui, &theme, &mut unit);
                    if unit != state.unit {
                        state.set_unit(unit);
                    }

                    // Saisie manuelle = format personnalisé.
                    let max = from_px(DOC_MAX_DIMENSION, state.unit);
                    let mut width = state.width;
                    ui_kit::components::NumberInput::new("Largeur")
                        .range(0.01..=max)
                        .stacked()
                        .show(ui, &theme, &mut width);
                    let mut height = state.height;
                    ui_kit::components::NumberInput::new("Hauteur")
                        .range(0.01..=max)
                        .stacked()
                        .show(ui, &theme, &mut height);
                    if width != state.width || height != state.height {
                        state.set_custom_size(width, height);
                    }

                    ui.horizontal(|ui| {
                        ui.label(body_text(&theme, "Orientation"));
                        let portrait = state.orientation == DocOrientation::Portrait;
                        if Button::new("Portrait")
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
                        if Button::new("Paysage")
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

    /// Entrée synthétique plein écran pour les tests de dialogue.
    fn test_input(screen: egui::Rect, time: f64) -> egui::RawInput {
        egui::RawInput {
            screen_rect: Some(screen),
            time: Some(time),
            ..Default::default()
        }
    }

    /// Régression : le bouton « Créer » est peint (dans le cadre de
    /// la fenêtre) et un clic synthétique dessus valide le dialogue
    /// (retourne les dimensions et referme).
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

        // Frame 1 : layout, on repère le texte « Créer » peint.
        // (Deux passes : la première peint en transparence à cause
        // du fondu d'apparition de la fenêtre.)
        for t in [0.0, 1.0] {
            ctx.run_ui(test_input(screen, t), |ui| {
                let c = ui.ctx().clone();
                let _ = draw_create_document_dialog(&c, &mut state);
            })
            .drop_without_applying_deltas();
        }
        let out = ctx.run_ui(test_input(screen, 1.0), |ui| {
            let c = ui.ctx().clone();
            let _ = draw_create_document_dialog(&c, &mut state);
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

        // Clic synthétique (appui + relâche) au centre du libellé.
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
                retour = draw_create_document_dialog(&c, &mut state);
            })
            .drop_without_applying_deltas();
        }
        // Défaut : A4 Portrait en cm → 2480 × 3508 px.
        assert_eq!(retour, Some(NewDocumentChoice::Create(2480, 3508)));
        assert!(!state.open, "la validation referme le dialogue");
    }

    /// Régression : le bouton « Ouvrir une image… » est peint et un
    /// clic dessus retourne le choix d'ouverture (et referme).
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
                let _ = draw_create_document_dialog(&c, &mut state);
            })
            .drop_without_applying_deltas();
        }
        let out = ctx.run_ui(test_input(screen, 1.0), |ui| {
            let c = ui.ctx().clone();
            let _ = draw_create_document_dialog(&c, &mut state);
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
                retour = draw_create_document_dialog(&c, &mut state);
            })
            .drop_without_applying_deltas();
        }
        assert_eq!(retour, Some(NewDocumentChoice::OpenImage));
        assert!(!state.open, "le choix referme le dialogue");
    }

    /// Régression : la fenêtre tient dans l'écran (contenu compact,
    /// pas de débordement horizontal).
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
            let _ = draw_create_document_dialog(&c, &mut state);
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
            let _ = draw_create_document_dialog(&ctx, &mut state);
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn impression_preset_switches_unit_to_cm() {
        let mut state = CreateDocumentDialogState::default();
        let preset = &active_presets(0)[0]; // A4 Portrait
        state.apply_preset(0, preset.px_w, preset.px_h, preset.unit);
        assert_eq!(state.unit, 2);
        // 2480 px à 300 DPI = ~21 cm (à epsilon près).
        assert!((state.width - 21.0).abs() < 0.1);
        assert_eq!(state.orientation, DocOrientation::Portrait);
        assert_eq!(state.px_dimensions(), (2480, 3508));
    }

    #[test]
    fn preset_cards_reflect_canvas_ratio() {
        let imp = active_presets(0);
        assert_eq!(imp[0].name, "A4 Portrait");
        assert!(imp[0].px_h > imp[0].px_w);
        let num = active_presets(1);
        assert_eq!(num[3].name, "Carré");
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
        // A4 ≈ 21 cm → passage en px : mêmes pixels.
        state.set_unit(0);
        assert_eq!(state.unit, 0);
        assert_eq!(state.px_dimensions(), (2480, 3508));
        assert!((state.width - 2480.0).abs() < 0.01);
        // Retour en cm : stable.
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
        // Dimensions échangées pour le portrait.
        assert!(state.height >= state.width);
    }
}

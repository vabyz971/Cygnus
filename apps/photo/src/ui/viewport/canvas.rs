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
#![allow(dead_code)] // TODO(Phase 5) : levé au câblage dans app.rs.
#![allow(unused_imports)] // TODO(Phase 5) : idem.

//! Canvas PHOTO : viewport générique + outils photo par-dessus.
//!
//! Widget métier : affiche la texture du `photo-engine` via
//! `ui_kit::Viewport`, mappe les outils photo (main, loupe, pinceau,
//! gomme, déplacement) et accumule les traits de dessin. À la fin
//! d'un trait (relâchement), la requête est RETOURNÉE dans
//! [`PhotoCanvasOutcome::paint`] : c'est l'app qui l'envoie au worker
//! (via [`PhotoAction`](crate::commands::PhotoAction)), jamais le
//! canvas. Pan/zoom restent 100% locaux (state-only).

use ui_kit::viewport::{Viewport, ViewportAction, ViewportState, ViewportTool};
use uuid::Uuid;

/// Outil actif du canvas photo (egui-side, sans dépendance iced).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoCanvasTool {
    /// Déplacement / sélection.
    #[default]
    Move,
    /// Main : le drag déplace la vue.
    Pan,
    /// Loupe : clic = zoom avant, clic droit = arrière.
    Zoom,
    /// Pinceau : le drag accumule un trait envoyé au moteur.
    Brush,
    /// Gomme : comme le pinceau, en mode effacement.
    Eraser,
    /// Pipette : le clic échantillonne la couleur (aperçu composite).
    Eyedropper,
}

impl PhotoCanvasTool {
    /// Outil viewport générique correspondant.
    pub fn viewport_tool(self) -> ViewportTool {
        match self {
            Self::Move => ViewportTool::Move,
            Self::Pan => ViewportTool::Pan,
            Self::Zoom => ViewportTool::Zoom,
            Self::Brush | Self::Eraser | Self::Eyedropper => ViewportTool::Brush,
        }
    }

    /// Vrai pour les outils accumulant un trait (pinceau, gomme).
    pub fn is_painting(self) -> bool {
        matches!(self, Self::Brush | Self::Eraser)
    }

    /// Aide contextuelle affichée dans la barre de statut (façon Affinity).
    pub fn hint(self) -> &'static str {
        match self {
            Self::Move => "Glisser : interagir — molette : zoom",
            Self::Pan => "Glisser : deplacer la vue — molette : zoom",
            Self::Zoom => "Clic : zoom avant — clic droit : zoom arriere",
            Self::Brush => "Glisser : peindre — relacher : commettre le trait",
            Self::Eraser => "Glisser : effacer — relacher : commettre",
            Self::Eyedropper => "Cliquer : echantillonner la couleur",
        }
    }
}

/// Réglages du pinceau/gomme (détenus par l'app).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhotoBrushSettings {
    /// Rayon en pixels image.
    pub radius: f32,
    /// Couleur RGB (ignorée en mode gomme).
    pub color: [u8; 3],
    /// Opacité 0..=1.
    pub opacity: f32,
}

impl Default for PhotoBrushSettings {
    fn default() -> Self {
        Self {
            radius: 8.0,
            color: [255, 255, 255],
            opacity: 1.0,
        }
    }
}

/// Requête de commit d'un trait (retournée à l'app, envoyée au
/// worker via `PhotoAction::CommitStroke`).
#[derive(Debug, Clone, PartialEq)]
pub struct PaintRequest {
    /// Calque pixels cible.
    pub layer: Uuid,
    /// Polyligne du trait (pixels image).
    pub points: Vec<(f32, f32)>,
    /// Vrai = gomme (réduit l'alpha), faux = pinceau.
    pub eraser: bool,
    /// Rayon en pixels image.
    pub radius: f32,
    /// Couleur RGB (ignorée en mode gomme).
    pub color: [u8; 3],
    /// Opacité du trait 0..=1.
    pub opacity: f32,
}

/// Canvas photo (builder) : texture moteur + outils par-dessus.
///
/// `stroke` accumule les positions monde du trait en cours (détenu
/// par l'app, vidé à chaque commit).
pub struct PhotoCanvas<'a> {
    texture: Option<(egui::TextureId, egui::Vec2)>,
    tool: PhotoCanvasTool,
    show_grid: bool,
    active_layer: Option<Uuid>,
    brush: PhotoBrushSettings,
    stroke: &'a mut Vec<egui::Vec2>,
}

impl<'a> PhotoCanvas<'a> {
    /// Crée un canvas photo (trait détenu par l'app).
    pub fn new(stroke: &'a mut Vec<egui::Vec2>) -> Self {
        Self {
            texture: None,
            tool: PhotoCanvasTool::default(),
            show_grid: false,
            active_layer: None,
            brush: PhotoBrushSettings::default(),
            stroke,
        }
    }

    /// Texture moteur + taille image (pixels). `None` = état vide.
    #[must_use]
    pub fn texture(mut self, texture: Option<(egui::TextureId, egui::Vec2)>) -> Self {
        self.texture = texture;
        self
    }

    /// Outil actif.
    #[must_use]
    pub fn tool(mut self, tool: PhotoCanvasTool) -> Self {
        self.tool = tool;
        self
    }

    /// Affiche/masque la grille.
    #[must_use]
    pub fn show_grid(mut self, show: bool) -> Self {
        self.show_grid = show;
        self
    }

    /// Calque pixels recevant les traits (`None` = dessin désactivé).
    #[must_use]
    pub fn active_layer(mut self, layer: Option<Uuid>) -> Self {
        self.active_layer = layer;
        self
    }

    /// Réglages pinceau/gomme.
    #[must_use]
    pub fn brush(mut self, brush: PhotoBrushSettings) -> Self {
        self.brush = brush;
        self
    }

    /// Affiche le canvas, applique pan/zoom, accumule puis commet les
    /// traits. Les positions rapportées sont en PIXELS IMAGE (prise en
    /// compte de l'ajustement, du zoom et du pan via `dest_rect`) —
    /// directement exploitables par le worker (transform identité).
    pub fn show(self, ui: &mut egui::Ui, state: &mut ViewportState) -> PhotoCanvasOutcome {
        let (texture_id, image_size) = self.texture.unzip();
        // Rectangle de destination AVANT interaction (même géométrie
        // que le dessin : disponible courant + zoom/pan).
        let dest = image_size.and_then(|size| {
            (size.x > 0.0 && size.y > 0.0).then(|| {
                Viewport::fit_dest(
                    ui.available_rect_before_wrap(),
                    size,
                    state.zoom(),
                    state.offset(),
                )
            })?
        });
        let response = Viewport::new()
            .texture(texture_id)
            .image_size(image_size.unwrap_or(egui::Vec2::ZERO))
            .tool(self.tool.viewport_tool())
            .show_grid(self.show_grid)
            .show(ui, state);

        let mut outcome = PhotoCanvasOutcome::default();
        // Écran → pixels image (ajustement + zoom + pan inversés).
        // Sans image (dest None), les positions sont ignorées.
        if let Some(dest) = dest {
            let scale = dest.width() / image_size.map_or(1.0, |size| size.x.max(1.0));
            if scale > 0.0 {
                for action in &response.actions {
                    if let ViewportAction::PointerAtWorld(world) = action {
                        let screen = state.world_to_screen(*world);
                        outcome.pointer_world.push(egui::vec2(
                            (screen.x - dest.min.x) / scale,
                            (screen.y - dest.min.y) / scale,
                        ));
                    }
                }
            }
        }
        if !self.tool.is_painting() {
            // Changement d'outil en cours de trait : abandon propre.
            // (Pipette et déplacement rapportent juste la position.)
            self.stroke.clear();
            return outcome;
        }
        self.stroke.extend(outcome.pointer_world.iter().copied());
        if response.response.drag_stopped() && !self.stroke.is_empty() {
            let points: Vec<(f32, f32)> = self.stroke.iter().map(|v| (v.x, v.y)).collect();
            self.stroke.clear();
            if let Some(layer) = self.active_layer {
                outcome.paint = Some(PaintRequest {
                    layer,
                    points,
                    eraser: self.tool == PhotoCanvasTool::Eraser,
                    radius: self.brush.radius,
                    color: self.brush.color,
                    opacity: self.brush.opacity,
                });
            }
        }
        outcome
    }
}

/// Résultat de [`PhotoCanvas::show`].
#[derive(Debug, Clone, Default)]
pub struct PhotoCanvasOutcome {
    /// Trait à commettre (`None` = rien ce frame ; l'app l'envoie).
    pub paint: Option<PaintRequest>,
    /// Positions monde du pointeur (clic/drag avec l'outil actif).
    pub pointer_world: Vec<egui::Vec2>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::PhotoEngineCommand;

    #[test]
    fn tool_mapping_and_painting_flags() {
        assert_eq!(PhotoCanvasTool::Move.viewport_tool(), ViewportTool::Move);
        assert_eq!(PhotoCanvasTool::Pan.viewport_tool(), ViewportTool::Pan);
        assert_eq!(PhotoCanvasTool::Zoom.viewport_tool(), ViewportTool::Zoom);
        assert_eq!(PhotoCanvasTool::Brush.viewport_tool(), ViewportTool::Brush);
        assert_eq!(PhotoCanvasTool::Eraser.viewport_tool(), ViewportTool::Brush);
        assert_eq!(
            PhotoCanvasTool::Eyedropper.viewport_tool(),
            ViewportTool::Brush
        );
        assert!(PhotoCanvasTool::Brush.is_painting());
        assert!(PhotoCanvasTool::Eraser.is_painting());
        assert!(!PhotoCanvasTool::Eyedropper.is_painting());
        assert!(!PhotoCanvasTool::Move.is_painting());
        assert!(!PhotoCanvasTool::Pan.is_painting());
        assert!(!PhotoCanvasTool::Zoom.is_painting());
    }

    #[test]
    fn canvas_renders_with_every_tool_without_panic() {
        let ctx = egui::Context::default();
        ui_kit::theme::setup_fonts(&ctx);
        for tool in [
            PhotoCanvasTool::Move,
            PhotoCanvasTool::Pan,
            PhotoCanvasTool::Zoom,
            PhotoCanvasTool::Brush,
            PhotoCanvasTool::Eraser,
            PhotoCanvasTool::Eyedropper,
        ] {
            let mut state = ViewportState::default();
            let mut stroke = Vec::new();
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    // Avec texture factice puis état vide.
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .texture(Some((egui::TextureId::User(7), egui::vec2(800.0, 600.0))))
                        .tool(tool)
                        .show_grid(true)
                        .active_layer(None)
                        .show(ui, &mut state);
                    assert!(outcome.paint.is_none(), "aucun trait sans interaction");
                    assert!(outcome.pointer_world.is_empty());
                    let outcome = PhotoCanvas::new(&mut stroke)
                        .tool(tool)
                        .show(ui, &mut state);
                    assert!(outcome.paint.is_none());
                });
            })
            .drop_without_applying_deltas();
        }
    }

    #[test]
    fn worker_paint_stroke_changes_pixels() {
        use crate::ui::engine_bridge::apply_command;
        use photo_engine::{Document, LayerNode, PixelLayer};
        use std::sync::Arc;

        let mut doc = Document::new(8, 8);
        let red = image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 40, 40, 255]));
        let id = {
            let layer = PixelLayer::new("fond", Arc::new(image::DynamicImage::ImageRgba8(red)));
            let id = layer.id;
            doc.push_layer(LayerNode::Pixel(layer));
            id
        };
        let points: Vec<(f32, f32)> = (0..8).map(|i| (i as f32, i as f32)).collect();
        apply_command(
            &mut doc,
            PhotoEngineCommand::PaintStroke {
                layer: id,
                points,
                eraser: false,
                radius: 3.0,
                color: [0, 0, 255],
                opacity: 1.0,
            },
        );
        let painted = doc
            .pixel_layer(id)
            .expect("calque present")
            .source_image
            .to_rgba8();
        // Le centre du trait diagonal est repeint en bleu opaque.
        assert_eq!(painted.get_pixel(4, 4).0[0..3], [0, 0, 255]);
        // Les coins hors trait sont intacts.
        assert_eq!(painted.get_pixel(0, 7).0[0..3], [200, 40, 40]);
    }

    #[test]
    fn worker_paint_invalid_is_noop() {
        use crate::ui::engine_bridge::apply_command;
        use photo_engine::{Document, LayerNode, PixelLayer};
        use std::sync::Arc;
        use uuid::Uuid;

        let mut doc = Document::new(4, 4);
        let red = image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 40, 40, 255]));
        doc.push_layer(LayerNode::Pixel(PixelLayer::new(
            "fond",
            Arc::new(image::DynamicImage::ImageRgba8(red)),
        )));
        // Id inconnu, rayon nul, aucun point : aucun panic, aucun changement.
        apply_command(
            &mut doc,
            PhotoEngineCommand::PaintStroke {
                layer: Uuid::new_v4(),
                points: vec![(1.0, 1.0)],
                eraser: false,
                radius: 2.0,
                color: [0, 0, 255],
                opacity: 1.0,
            },
        );
        let id = doc.root[0].id();
        apply_command(
            &mut doc,
            PhotoEngineCommand::PaintStroke {
                layer: id,
                points: vec![],
                eraser: false,
                radius: 2.0,
                color: [0, 0, 255],
                opacity: 1.0,
            },
        );
        apply_command(
            &mut doc,
            PhotoEngineCommand::PaintStroke {
                layer: id,
                points: vec![(1.0, 1.0)],
                eraser: false,
                radius: 0.0,
                color: [0, 0, 255],
                opacity: 1.0,
            },
        );
        let untouched = doc
            .pixel_layer(id)
            .expect("calque present")
            .source_image
            .to_rgba8();
        assert!(untouched.pixels().all(|p| p.0[0..3] == [200, 40, 40]));
    }
}

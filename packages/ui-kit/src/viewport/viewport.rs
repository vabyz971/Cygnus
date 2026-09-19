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

//! Widget viewport canvas : affiche une texture moteur, gère pan/zoom.
//!
//! Le widget ne fait QU'afficher la texture (id détenu par l'app) et
//! router les interactions souris : aucun décodage, aucun rendu lourd
//! ici. Contrat moteur (câblé en Phase 5 côté app) :
//! 1. le moteur rend sur un thread background et envoie le résultat
//!    (`egui::ColorImage`, ou texture wgpu native) via un channel ;
//! 2. l'app poll le channel chaque frame (non bloquant) et nourrit
//!    [`ViewportTextureCache`] (ou enregistre la texture native via
//!    `register_native_texture` d'eframe pour le zéro-copie GPU) ;
//! 3. l'id est passé à [`Viewport`] qui l'applique au draw avec
//!    zoom/pan/grille (modèle « state-only »).

use super::viewport_interaction::{ViewportAction, ViewportTool, handle_pointer};
use super::viewport_state::ViewportState;
use crate::theme::CygnusTheme;

/// UV couvrant toute la texture.
fn full_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// Charge (ou met à jour) une texture egui depuis une image CPU.
///
/// Le `TextureHandle` retourné doit être conservé vivant par l'app
/// (ex. dans [`ViewportTextureCache`]) tant que la texture est affichée.
pub fn load_texture(
    ctx: &egui::Context,
    name: &str,
    image: egui::ColorImage,
    linear_filter: bool,
) -> egui::TextureHandle {
    let options = if linear_filter {
        egui::TextureOptions::LINEAR
    } else {
        egui::TextureOptions::NEAREST
    };
    ctx.load_texture(name, image, options)
}

/// Cache applicatif d'une texture canvas (id + taille image).
///
/// L'app en détient une instance, l'alimente depuis son channel moteur
/// (non bloquant, à chaque nouveau rendu), et passe l'id au widget.
/// La texture GPU n'est allouée qu'une fois : les images suivantes de
/// mêmes dimensions mettent à jour le slot existant via
/// [`egui::TextureHandle::set`] (aucun free/alloc GPU) ; un changement
/// de dimensions recrée proprement la texture. Zoom/pan/grille et les
/// simples repaints ne touchent jamais aux pixels ni à la texture.
#[derive(Default)]
pub struct ViewportTextureCache {
    handle: Option<egui::TextureHandle>,
    size: egui::Vec2,
}

impl ViewportTextureCache {
    /// Crée un cache vide (aucune image).
    pub fn new() -> Self {
        Self::default()
    }

    /// Met à jour l'image affichée.
    /// À appeler uniquement à réception d'un nouveau rendu moteur.
    ///
    /// - première image : allocation via `ctx.load_texture()` ;
    /// - mêmes dimensions que la précédente : mise à jour en place
    ///   (`TextureHandle::set()`, id stable, aucun realloc GPU) ;
    /// - dimensions différentes : l'ancien handle est libéré et une
    ///   nouvelle texture est allouée.
    ///
    /// Le `ColorImage` fourni est consommé tel quel (aucune copie ni
    /// conversion supplémentaire ici) : la seule conversion RGBA →
    /// `ColorImage` a lieu une fois côté appelant, par nouveau rendu.
    pub fn update(&mut self, ctx: &egui::Context, name: &str, image: egui::ColorImage) {
        let size = egui::vec2(image.width() as f32, image.height() as f32);
        let same_size = self.handle.is_some() && self.size == size;
        if same_size {
            if let Some(handle) = self.handle.as_mut() {
                handle.set(image, egui::TextureOptions::LINEAR);
            }
        } else {
            // Première image ou changement de dimensions : (re)création
            // (l'ancien handle éventuel est libéré au remplacement).
            self.handle = Some(load_texture(ctx, name, image, true));
        }
        self.size = size;
    }

    /// Vide le cache (fermeture de document) : le handle est libéré
    /// (delta `free` egui) et la prochaine image réallouera.
    pub fn clear(&mut self) {
        self.handle = None;
        self.size = egui::Vec2::ZERO;
    }

    /// Id de texture et taille image, si une image est chargée.
    /// Stable entre deux `update()` de mêmes dimensions (aucune
    /// nouvelle texture GPU sur simple repaint : `show()` ne fait que
    /// relire cet id).
    pub fn texture(&self) -> Option<(egui::TextureId, egui::Vec2)> {
        self.handle.as_ref().map(|handle| (handle.id(), self.size))
    }
}

/// Widget viewport canvas (texture moteur + interactions).
///
/// # Exemple
/// ```rust,no_run
/// # use ui_kit::viewport::{Viewport, ViewportState, ViewportTextureCache};
/// # egui::__run_test_ui(|ui| {
/// let cache = ViewportTextureCache::new();
/// let mut viewport = ViewportState::default();
/// let (texture, size) = (None, egui::vec2(800.0, 600.0));
/// let canvas = Viewport::new()
///     .texture(texture)
///     .image_size(size)
///     .show_grid(true);
/// let response = canvas.show(ui, &mut viewport);
/// # });
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    texture_id: Option<egui::TextureId>,
    image_size: egui::Vec2,
    tool: ViewportTool,
    show_grid: bool,
}

impl Default for Viewport {
    fn default() -> Self {
        Self::new()
    }
}

impl Viewport {
    /// Crée un canvas sans texture (état vide).
    pub fn new() -> Self {
        Self {
            texture_id: None,
            image_size: egui::Vec2::ZERO,
            tool: ViewportTool::default(),
            show_grid: false,
        }
    }

    /// Texture à afficher (`None` = état vide).
    #[must_use]
    pub fn texture(mut self, texture_id: Option<egui::TextureId>) -> Self {
        self.texture_id = texture_id;
        self
    }

    /// Taille en pixels de l'image source (pour l'ajustement + le pan).
    #[must_use]
    pub fn image_size(mut self, size: egui::Vec2) -> Self {
        self.image_size = size;
        self
    }

    /// Outil actif (pan, loupe, pinceau…).
    #[must_use]
    pub fn tool(mut self, tool: ViewportTool) -> Self {
        self.tool = tool;
        self
    }

    /// Affiche/masque la grille.
    #[must_use]
    pub fn show_grid(mut self, show: bool) -> Self {
        self.show_grid = show;
        self
    }

    /// Rectangle de destination : image ajustée au viewport, centrée,
    /// mise à l'échelle par le zoom et décalée par le pan.
    /// `None` si la taille image est invalide. Exposée pour que les
    /// apps convertissent écran→pixels image (outils de dessin).
    pub fn fit_dest(
        available: egui::Rect,
        image_size: egui::Vec2,
        zoom: f32,
        offset: egui::Vec2,
    ) -> Option<egui::Rect> {
        if image_size.x <= 0.0 || image_size.y <= 0.0 {
            return None;
        }
        let fit = (available.width() / image_size.x)
            .min(available.height() / image_size.y)
            .max(0.0);
        let size = image_size * fit * zoom;
        let center = available.center() + offset;
        Some(egui::Rect::from_center_size(center, size))
    }

    /// Dessine la grille dans `rect` (pas écran de 32px, plafonnée).
    fn paint_grid(ui: &mut egui::Ui, rect: egui::Rect) {
        let theme = CygnusTheme::dark();
        let stroke = egui::Stroke::new(1.0, theme.colors.border.linear_multiply(0.5));
        let step = 32.0;
        let vertical = ((rect.width() / step) as usize).min(100);
        let horizontal = ((rect.height() / step) as usize).min(100);
        for i in 0..=vertical {
            let x = rect.min.x + i as f32 * step;
            ui.painter().line_segment(
                [egui::pos2(x, rect.min.y), egui::pos2(x, rect.max.y)],
                stroke,
            );
        }
        for i in 0..=horizontal {
            let y = rect.min.y + i as f32 * step;
            ui.painter().line_segment(
                [egui::pos2(rect.min.x, y), egui::pos2(rect.max.x, y)],
                stroke,
            );
        }
    }

    /// Affiche le canvas et applique les interactions au `state`.
    pub fn show(self, ui: &mut egui::Ui, state: &mut ViewportState) -> ViewportResponse {
        let theme = CygnusTheme::dark();
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        ui.painter()
            .rect_filled(rect, 0.0, theme.colors.bg_tertiary);

        match self.texture_id {
            Some(texture_id) => {
                if let Some(dest) =
                    Self::fit_dest(rect, self.image_size, state.zoom(), state.offset())
                {
                    ui.painter()
                        .image(texture_id, dest, full_uv(), egui::Color32::WHITE);
                    if self.show_grid {
                        Self::paint_grid(ui, dest);
                    }
                }
            }
            None => {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Aucune image",
                    egui::FontId::proportional(theme.typography.body_size),
                    theme.colors.fg_secondary,
                );
            }
        }

        let actions = handle_pointer(ui, &response, state, self.tool, rect.center());
        ViewportResponse {
            response: response.on_hover_cursor(self.tool.cursor()),
            actions,
        }
    }
}

/// Réponse de [`Viewport::show`].
pub struct ViewportResponse {
    /// Réponse egui du viewport.
    pub response: egui::Response,
    /// Actions d'interaction (pan, zoom, positions monde).
    pub actions: Vec<ViewportAction>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_image() -> egui::ColorImage {
        egui::ColorImage::new([4, 4], vec![egui::Color32::from_rgb(200, 40, 40); 16])
    }

    #[test]
    fn canvas_empty_renders_without_panic() {
        let ctx = egui::Context::default();
        let mut viewport = ViewportState::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let response = Viewport::new().show_grid(true).show(ui, &mut viewport);
                assert!(response.actions.is_empty());
            });
        })
        .drop_without_applying_deltas();
    }

    #[test]
    fn canvas_with_texture_renders_without_panic() {
        // Texture factice (id managé bidon) : aucun GPU requis en
        // headless — le widget ne fait qu'émettre la primitive image.
        // Le chemin GPU réel (register_native_texture) est câblé en
        // Phase 5 côté app (eframe).
        let ctx = egui::Context::default();
        let mut viewport = ViewportState::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = Viewport::new()
                    .texture(Some(egui::TextureId::Managed(0)))
                    .image_size(egui::vec2(800.0, 600.0))
                    .tool(ViewportTool::Pan)
                    .show_grid(true)
                    .show(ui, &mut viewport);
            });
        });
        assert!(!output.shapes.is_empty(), "aucune primitive dessinee");
        output.drop_without_applying_deltas();
    }

    #[test]
    fn texture_cache_update_and_clear() {
        let ctx = egui::Context::default();
        let mut cache = ViewportTextureCache::new();
        assert!(cache.texture().is_none());
        cache.update(&ctx, "test_canvas", test_image());
        let (id, size) = cache.texture().expect("texture chargee");
        assert_eq!(size, egui::vec2(4.0, 4.0));
        let _ = id;
        // Afficher la texture du cache sans panic.
        let mut viewport = ViewportState::default();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                let _ = Viewport::new()
                    .texture(cache.texture().map(|(id, _)| id))
                    .image_size(size)
                    .show(ui, &mut viewport);
            });
        })
        .drop_without_applying_deltas();
        cache.clear();
        assert!(cache.texture().is_none());
    }

    fn solid_image(width: usize, height: usize, color: egui::Color32) -> egui::ColorImage {
        egui::ColorImage::new([width, height], vec![color; width * height])
    }

    #[test]
    fn texture_id_stable_on_same_size_update() {
        // Deuxième image de mêmes dimensions : mise à jour en place,
        // aucun nouveau slot GPU (id stable).
        let ctx = egui::Context::default();
        let mut cache = ViewportTextureCache::new();
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::RED));
        let (first_id, first_size) = cache.texture().expect("texture chargee");
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::BLUE));
        let (second_id, second_size) = cache.texture().expect("texture chargee");
        assert_eq!(first_id, second_id, "mêmes dimensions = même texture");
        assert_eq!(second_size, egui::vec2(4.0, 4.0));
        assert_eq!(first_size, second_size);
        // Troisième update identique : toujours stable.
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::GREEN));
        assert_eq!(cache.texture().expect("texture").0, first_id);
    }

    #[test]
    fn texture_recreated_on_dimension_change() {
        // Changement de dimensions : l'ancien slot est abandonné et une
        // nouvelle texture est allouée (nouvel id, nouvelle taille).
        let ctx = egui::Context::default();
        let mut cache = ViewportTextureCache::new();
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::RED));
        let (small_id, _) = cache.texture().expect("texture chargee");
        cache.update(&ctx, "test_canvas", solid_image(8, 6, egui::Color32::RED));
        let (big_id, big_size) = cache.texture().expect("texture chargee");
        assert_ne!(
            small_id, big_id,
            "dimensions différentes = nouvelle texture"
        );
        assert_eq!(big_size, egui::vec2(8.0, 6.0));
        // Retour aux dimensions précédentes : realloc à nouveau (l'ancien
        // slot 4x4 a été libéré), puis stabilité.
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::RED));
        let (back_id, back_size) = cache.texture().expect("texture chargee");
        assert_ne!(back_id, big_id);
        assert_eq!(back_size, egui::vec2(4.0, 4.0));
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::BLUE));
        assert_eq!(cache.texture().expect("texture").0, back_id);
    }

    #[test]
    fn clear_then_update_reallocates() {
        // `clear()` libère le handle : l'update suivant réalloue même à
        // dimensions identiques (l'id libéré n'est pas réutilisé).
        let ctx = egui::Context::default();
        let mut cache = ViewportTextureCache::new();
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::RED));
        let (before_id, _) = cache.texture().expect("texture chargee");
        cache.clear();
        assert!(cache.texture().is_none());
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::RED));
        let (after_id, after_size) = cache.texture().expect("texture chargee");
        assert_ne!(before_id, after_id, "après clear : nouvelle allocation");
        assert_eq!(after_size, egui::vec2(4.0, 4.0));
    }

    #[test]
    fn repaint_and_zoom_pan_keep_texture_id() {
        // Sans appel à `update()`, ni les repaints ni zoom/pan/grille ne
        // créent de texture : l'id reste stable sur plusieurs frames.
        let ctx = egui::Context::default();
        let mut cache = ViewportTextureCache::new();
        cache.update(&ctx, "test_canvas", solid_image(64, 48, egui::Color32::RED));
        let (expected_id, size) = cache.texture().expect("texture chargee");
        let mut viewport = ViewportState::default();
        for _ in 0..3 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    let _ = Viewport::new()
                        .texture(cache.texture().map(|(id, _)| id))
                        .image_size(size)
                        .tool(ViewportTool::Pan)
                        .show_grid(true)
                        .show(ui, &mut viewport);
                });
            })
            .drop_without_applying_deltas();
            viewport.zoom_by(1.5, None);
            viewport.pan_by(egui::vec2(10.0, -5.0));
        }
        assert_eq!(cache.texture().expect("texture").0, expected_id);
    }

    #[test]
    fn dest_rect_fits_and_zooms() {
        let available = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 300.0));
        // Ajustement parfait à zoom 1 : 400x300.
        let dest = Viewport::fit_dest(available, egui::vec2(800.0, 600.0), 1.0, egui::Vec2::ZERO)
            .expect("dest calcule");
        assert!((dest.width() - 400.0).abs() < 1e-3);
        assert!((dest.height() - 300.0).abs() < 1e-3);
        // Zoom 2x : dimensions doublées.
        let dest = Viewport::fit_dest(available, egui::vec2(800.0, 600.0), 2.0, egui::Vec2::ZERO)
            .expect("dest calcule");
        assert!((dest.width() - 800.0).abs() < 1e-3);
        // Taille invalide : rien à dessiner.
        assert!(Viewport::fit_dest(available, egui::Vec2::ZERO, 1.0, egui::Vec2::ZERO).is_none());
    }
}

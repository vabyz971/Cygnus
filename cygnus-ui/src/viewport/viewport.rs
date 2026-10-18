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

//! Canvas viewport widget: displays an engine texture, handles pan/zoom.
//!
//! The widget ONLY displays the texture (id owned by the app) and
//! routes mouse interactions: no decoding, no heavy rendering here.
//! Engine contract (wired in Phase 5 on the app side):
//! 1. the engine renders on a background thread and sends the result
//!    (`egui::ColorImage`, or native wgpu texture) through a channel;
//! 2. the app polls the channel every frame (non-blocking) and feeds
//!    [`ViewportTextureCache`] (or registers the native texture via
//!    eframe's `register_native_texture` for zero-copy GPU);
//! 3. the id is passed to [`Viewport`], which applies it at draw time
//!    with zoom/pan/grid ("state-only" model).

use super::viewport_interaction::{ViewportAction, ViewportTool, handle_pointer};
use super::viewport_state::ViewportState;
use crate::theme::UiThemeExt;

/// UV covering the whole texture.
fn full_uv() -> egui::Rect {
    egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0))
}

/// Loads (or updates) an egui texture from a CPU image.
///
/// The returned `TextureHandle` must be kept alive by the app (e.g.
/// inside [`ViewportTextureCache`]) while the texture is displayed.
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

/// App-side cache of a canvas texture (id + image size).
///
/// The app owns an instance, feeds it from its engine channel
/// (non-blocking, on every new render), and passes the id to the widget.
/// The GPU texture is allocated only once: subsequent same-size images
/// update the existing slot via [`egui::TextureHandle::set`] (no GPU
/// free/alloc); a size change cleanly recreates the texture. Zoom/pan/grid
/// and mere repaints never touch pixels or the texture.
#[derive(Default)]
pub struct ViewportTextureCache {
    handle: Option<egui::TextureHandle>,
    size: egui::Vec2,
}

impl ViewportTextureCache {
    /// Creates an empty cache (no image).
    pub fn new() -> Self {
        Self::default()
    }

    /// Updates the displayed image. Call only on receiving a new
    /// engine render.
    ///
    /// - first image: allocation via `ctx.load_texture()`;
    /// - same size as the previous one: in-place update
    ///   (`TextureHandle::set()`, stable id, no GPU realloc);
    /// - different sizes: the old handle is freed and a new texture
    ///   is allocated.
    ///
    /// The provided `ColorImage` is consumed as-is (no extra copy or
    /// conversion here): the single RGBA → `ColorImage` conversion happens
    /// once on the caller side, per new render.
    pub fn update(&mut self, ctx: &egui::Context, name: &str, image: egui::ColorImage) {
        let size = egui::vec2(image.width() as f32, image.height() as f32);
        let same_size = self.handle.is_some() && self.size == size;
        if same_size {
            if let Some(handle) = self.handle.as_mut() {
                handle.set(image, egui::TextureOptions::LINEAR);
            }
        } else {
            // First image or size change: (re)creation (any previous
            // handle is freed on replacement).
            self.handle = Some(load_texture(ctx, name, image, true));
        }
        self.size = size;
    }

    /// Empties the cache (document close): the handle is freed (egui
    /// `free` delta) and the next image will reallocate.
    pub fn clear(&mut self) {
        self.handle = None;
        self.size = egui::Vec2::ZERO;
    }

    /// Texture id and image size, when an image is loaded. Stable
    /// across two same-size `update()` calls (no new GPU texture on a
    /// mere repaint: `show()` only re-reads this id).
    pub fn texture(&self) -> Option<(egui::TextureId, egui::Vec2)> {
        self.handle.as_ref().map(|handle| (handle.id(), self.size))
    }
}

/// Canvas viewport widget (engine texture + interactions).
///
/// # Example
/// ```rust,no_run
/// # use cygnus_ui::viewport::{Viewport, ViewportState, ViewportTextureCache};
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
    /// Creates a texture-less canvas (empty state).
    pub fn new() -> Self {
        Self {
            texture_id: None,
            image_size: egui::Vec2::ZERO,
            tool: ViewportTool::default(),
            show_grid: false,
        }
    }

    /// Texture to display (`None` = empty state).
    #[must_use]
    pub fn texture(mut self, texture_id: Option<egui::TextureId>) -> Self {
        self.texture_id = texture_id;
        self
    }

    /// Source-image size in pixels (for fit + pan).
    #[must_use]
    pub fn image_size(mut self, size: egui::Vec2) -> Self {
        self.image_size = size;
        self
    }

    /// Active tool (pan, magnifier, brush…).
    #[must_use]
    pub fn tool(mut self, tool: ViewportTool) -> Self {
        self.tool = tool;
        self
    }

    /// Shows/hides the grid.
    #[must_use]
    pub fn show_grid(mut self, show: bool) -> Self {
        self.show_grid = show;
        self
    }

    /// Destination rectangle: image fitted to the viewport, centered,
    /// scaled by the zoom and shifted by the pan. `None` when the image
    /// size is invalid. Exposed so apps convert screen→image pixels
    /// (drawing tools).
    pub fn fit_dest(
        available: egui::Rect,
        image_size: egui::Vec2,
        zoom: f32,
        offset: egui::Vec2,
    ) -> Option<egui::Rect> {
        Self::fit_dest_with_margin(available, image_size, zoom, offset, 0.0)
    }

    /// [`Self::fit_dest`] keeping `margin` screen pixels free around the
    /// fitted image at zoom 1 (breathing room, Affinity-style); the
    /// center and the zoom/pan behavior are unchanged.
    pub fn fit_dest_with_margin(
        available: egui::Rect,
        image_size: egui::Vec2,
        zoom: f32,
        offset: egui::Vec2,
        margin: f32,
    ) -> Option<egui::Rect> {
        if image_size.x <= 0.0 || image_size.y <= 0.0 {
            return None;
        }
        let room = available.shrink(margin.max(0.0));
        let fit = (room.width() / image_size.x)
            .min(room.height() / image_size.y)
            .max(0.0);
        let size = image_size * fit * zoom;
        let center = available.center() + offset;
        Some(egui::Rect::from_center_size(center, size))
    }

    /// Draws the grid inside `rect` (32px screen step, capped).
    fn paint_grid(ui: &mut egui::Ui, rect: egui::Rect) {
        let theme = ui.cygnus_theme();
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

    /// Displays the canvas and applies interactions to `state`.
    pub fn show(self, ui: &mut egui::Ui, state: &mut ViewportState) -> ViewportResponse {
        let theme = ui.cygnus_theme();
        let (rect, response) =
            ui.allocate_exact_size(ui.available_size(), egui::Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, theme.colors.bg_canvas);

        match self.texture_id {
            Some(texture_id) => {
                if let Some(dest) = Self::fit_dest_with_margin(
                    rect,
                    self.image_size,
                    state.zoom(),
                    state.offset(),
                    theme.spacing.xl,
                ) {
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

/// [`Viewport::show`] response.
pub struct ViewportResponse {
    /// The viewport's egui response.
    pub response: egui::Response,
    /// Interaction actions (pan, zoom, world positions).
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
        // Dummy texture (bogus managed id): no GPU needed headless — the
        // widget only emits the image primitive. The real GPU path
        // (register_native_texture) is wired in Phase 5 on the app side
        // (eframe).
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
        // Display the cache's texture without panic.
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
        // Second same-size image: in-place update, no new GPU slot
        // (stable id).
        let ctx = egui::Context::default();
        let mut cache = ViewportTextureCache::new();
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::RED));
        let (first_id, first_size) = cache.texture().expect("texture chargee");
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::BLUE));
        let (second_id, second_size) = cache.texture().expect("texture chargee");
        assert_eq!(first_id, second_id, "mêmes dimensions = même texture");
        assert_eq!(second_size, egui::vec2(4.0, 4.0));
        assert_eq!(first_size, second_size);
        // Third identical update: still stable.
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::GREEN));
        assert_eq!(cache.texture().expect("texture").0, first_id);
    }

    #[test]
    fn texture_recreated_on_dimension_change() {
        // Size change: the old slot is dropped and a new texture is
        // allocated (new id, new size).
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
        // Back to the previous sizes: realloc again (the old 4x4 slot was
        // freed), then stability.
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::RED));
        let (back_id, back_size) = cache.texture().expect("texture chargee");
        assert_ne!(back_id, big_id);
        assert_eq!(back_size, egui::vec2(4.0, 4.0));
        cache.update(&ctx, "test_canvas", solid_image(4, 4, egui::Color32::BLUE));
        assert_eq!(cache.texture().expect("texture").0, back_id);
    }

    #[test]
    fn clear_then_update_reallocates() {
        // `clear()` frees the handle: the next update reallocates even at
        // identical sizes (the freed id is not reused).
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
        // Without any `update()` call, neither repaints nor zoom/pan/grid
        // create a texture: the id stays stable across frames.
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
        // Perfect fit at zoom 1: 400x300.
        let dest = Viewport::fit_dest(available, egui::vec2(800.0, 600.0), 1.0, egui::Vec2::ZERO)
            .expect("dest calcule");
        assert!((dest.width() - 400.0).abs() < 1e-3);
        assert!((dest.height() - 300.0).abs() < 1e-3);
        // 2x zoom: doubled dimensions.
        let dest = Viewport::fit_dest(available, egui::vec2(800.0, 600.0), 2.0, egui::Vec2::ZERO)
            .expect("dest calcule");
        assert!((dest.width() - 800.0).abs() < 1e-3);
        // Invalid size: nothing to draw.
        assert!(Viewport::fit_dest(available, egui::Vec2::ZERO, 1.0, egui::Vec2::ZERO).is_none());
        // Margin: the fit leaves room on the limiting axis, same center.
        let framed = Viewport::fit_dest_with_margin(
            available,
            egui::vec2(800.0, 600.0),
            1.0,
            egui::Vec2::ZERO,
            20.0,
        )
        .expect("dest");
        assert_eq!(framed.center(), available.center());
        // 360x260 room: height limits (260 / 600), width = 800 × that.
        assert!((framed.height() - 260.0).abs() < 1e-3);
        assert!((framed.width() - 800.0 * 260.0 / 600.0).abs() < 1e-3);
    }
}

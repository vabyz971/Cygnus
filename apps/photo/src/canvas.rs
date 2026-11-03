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

//! The canvas of Cygnus Photo: the open document, its view (zoom and
//! position) and the egui-wgpu callback that draws it with the renderer
//! (docs/decisions/0012).
//!
//! The document only changes through its command bus. Each frame, the
//! callback carries an immutable snapshot of the document and the changes
//! since the revision the canvas last showed, so that only those tiles are
//! composited again. If the canvas missed a revision, it is told that
//! everything changed.

use cygnus_photo_core::resolve::{Resolvers, resolve_and_execute};
use cygnus_photo_io::FileImporter;
use std::sync::Arc;

use cygnus_commands::{ChangeSet, CommandBus, Revision, Source};
use cygnus_photo_core::script::{SCRIPT_VERSION, Script};
use cygnus_photo_core::{CoreError, Document, PhotoChanges, PhotoCommand};
use cygnus_photo_render::{Canvas, CanvasView, RenderError, Renderer, gpu};
use cygnus_ui_kit::theme::palette;
use eframe::egui_wgpu;
use egui::{PointerButton, Rect, Sense, Ui};

/// Zoom limits, in screen pixels per document pixel.
const MIN_ZOOM: f32 = 0.01;
const MAX_ZOOM: f32 = 32.0;
/// Zoom factor of the zoom in and out buttons.
const ZOOM_STEP: f32 = 1.25;
/// Share of the canvas the document fills after "fit to screen".
const FIT_MARGIN: f32 = 0.92;
/// Side of a transparency checker square, in screen pixels.
const CHECKER_SIZE: f32 = 8.0;

/// GPU resources of the canvas, stored in egui-wgpu's callback resources.
pub struct CanvasResources {
    renderer: Renderer,
    canvas: Canvas,
    shown: Option<Revision>,
    error: Option<String>,
}

/// Creates the canvas resources on eframe's device.
pub fn install(render_state: &egui_wgpu::RenderState) -> Result<(), RenderError> {
    let renderer = Renderer::new(&render_state.device, &render_state.queue)?;
    let budget = gpu::cache_memory_budget(render_state.adapter.get_info().device_type);
    let canvas = Canvas::new(&renderer, render_state.target_format, budget)?;
    render_state
        .renderer
        .write()
        .callback_resources
        .insert(CanvasResources {
            renderer,
            canvas,
            shown: None,
            error: None,
        });
    Ok(())
}

/// Work counters and cache size of the canvas, for the benchmark.
pub fn stats(
    render_state: &egui_wgpu::RenderState,
) -> Option<(
    cygnus_photo_render::CanvasStats,
    u32,
    cygnus_photo_render::SourceStats,
)> {
    render_state
        .renderer
        .read()
        .callback_resources
        .get::<CanvasResources>()
        .map(|resources| {
            (
                resources.canvas.stats(),
                resources.canvas.capacity(),
                resources.renderer.source_stats(),
            )
        })
}

/// Last canvas error, if the canvas could not be drawn.
pub fn last_error(render_state: &egui_wgpu::RenderState) -> Option<String> {
    render_state
        .renderer
        .read()
        .callback_resources
        .get::<CanvasResources>()
        .and_then(|resources| resources.error.clone())
}

fn rgb(color: egui::Color32) -> [f32; 3] {
    [color.r(), color.g(), color.b()].map(|c| f32::from(c) / 255.0)
}

struct CanvasCallback {
    document: Arc<Document>,
    revision: Revision,
    base: Revision,
    changes: PhotoChanges,
    view: CanvasView,
}

impl egui_wgpu::CallbackTrait for CanvasCallback {
    fn prepare(
        &self,
        _device: &eframe::wgpu::Device,
        _queue: &eframe::wgpu::Queue,
        _screen: &egui_wgpu::ScreenDescriptor,
        _encoder: &mut eframe::wgpu::CommandEncoder,
        resources: &mut egui_wgpu::CallbackResources,
    ) -> Vec<eframe::wgpu::CommandBuffer> {
        let Some(resources) = resources.get_mut::<CanvasResources>() else {
            return Vec::new();
        };
        let everything = PhotoChanges::everything();
        let changes = if resources.shown == Some(self.base) {
            &self.changes
        } else {
            &everything
        };
        match resources
            .canvas
            .prepare(&resources.renderer, &self.document, changes, &self.view)
        {
            Ok(()) => {
                resources.shown = Some(self.revision);
                resources.error = None;
            }
            Err(error) => {
                if resources.error.is_none() {
                    eprintln!("cygnus-photo: canvas: {error}");
                }
                resources.shown = None;
                resources.error = Some(error.to_string());
            }
        }
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut eframe::wgpu::RenderPass<'static>,
        resources: &egui_wgpu::CallbackResources,
    ) {
        if let Some(resources) = resources.get::<CanvasResources>()
            && resources.error.is_none()
        {
            resources.canvas.paint(pass);
        }
    }
}

/// An open document with its bus and its view.
pub struct Session {
    bus: CommandBus<PhotoCommand>,
    document: Document,
    snapshot: Arc<Document>,
    sent: Revision,
    /// Screen pixels per document pixel.
    zoom: f32,
    /// Document point at the center of the canvas.
    center: [f32; 2],
    /// Fit the document on the next frame (size of the canvas unknown yet).
    fit_pending: bool,
}

impl Session {
    /// Opens a document built by `commands`, issued by `source`.
    pub fn from_commands(commands: Vec<PhotoCommand>, source: Source) -> Self {
        let script = Script {
            version: SCRIPT_VERSION,
            commands,
        };
        let mut document = Document::default();
        let mut bus = CommandBus::new();
        if let Err(error) = script.run(&mut bus, &mut document, source) {
            eprintln!("cygnus-photo: document: {error}");
        }
        Self {
            snapshot: Arc::new(document.clone()),
            document,
            bus,
            sent: Revision::ZERO,
            zoom: 1.0,
            center: [0.0, 0.0],
            fit_pending: true,
        }
    }

    /// Opens an already built document.
    pub fn from_document(document: Document) -> Self {
        Self {
            snapshot: Arc::new(document.clone()),
            document,
            bus: CommandBus::new(),
            sent: Revision::ZERO,
            zoom: 1.0,
            center: [0.0, 0.0],
            fit_pending: true,
        }
    }

    /// Applies `command` to the document, through the single path of the
    /// resolved commands (docs/decisions/0020). Files are read relative to
    /// the working folder; merging layers needs the renderer of the window
    /// ([`Self::execute_with`]).
    pub fn execute(&mut self, command: PhotoCommand, source: Source) -> Result<(), CoreError> {
        let importer = FileImporter::new(".");
        let resolvers = Resolvers {
            import: Some(&importer),
            merge: None,
        };
        self.execute_with(command, source, &resolvers)
    }

    /// Applies `command` with the given reader and renderer.
    pub fn execute_with(
        &mut self,
        command: PhotoCommand,
        source: Source,
        resolvers: &Resolvers<'_>,
    ) -> Result<(), CoreError> {
        resolve_and_execute(
            &mut self.bus,
            &mut self.document,
            command,
            source,
            resolvers,
        )
        .map(|_| ())
    }

    /// Places the view: `zoom` screen pixels per document pixel, `center`
    /// document point at the center of the canvas.
    pub fn set_camera(&mut self, zoom: f32, center: [f32; 2]) {
        self.zoom = zoom.clamp(MIN_ZOOM, MAX_ZOOM);
        self.center = center;
        self.fit_pending = false;
    }

    /// The document.
    pub fn document(&self) -> &Document {
        &self.document
    }

    /// Current zoom, 1.0 being 100 %.
    pub fn zoom(&self) -> f32 {
        self.zoom
    }

    /// Fits the whole document in the canvas on the next frame.
    pub fn fit(&mut self) {
        self.fit_pending = true;
    }

    /// Shows the document at 100 %: one document pixel per screen pixel.
    pub fn actual_size(&mut self) {
        self.zoom = 1.0;
        self.fit_pending = false;
    }

    /// Zooms in or out by one step around the canvas center.
    pub fn zoom_step(&mut self, steps: i32) {
        self.zoom = (self.zoom * ZOOM_STEP.powi(steps)).clamp(MIN_ZOOM, MAX_ZOOM);
        self.fit_pending = false;
    }

    /// Shows the canvas in `ui` and handles zoom and pan. `pan_tool` is true
    /// when the pan tool is active (primary button pans).
    pub fn show(&mut self, ui: &mut Ui, pan_tool: bool) {
        let rect = ui.max_rect();
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let ppp = ui.ctx().pixels_per_point();
        let size_px = [rect.width() * ppp, rect.height() * ppp];
        let (width, height) = (self.document.width as f32, self.document.height as f32);

        if self.fit_pending && size_px[0] > 0.0 && size_px[1] > 0.0 {
            self.zoom = (FIT_MARGIN * (size_px[0] / width).min(size_px[1] / height))
                .clamp(MIN_ZOOM, MAX_ZOOM);
            self.center = [width / 2.0, height / 2.0];
            self.fit_pending = false;
        }

        // Pan: middle button, Space + primary button, or the pan tool.
        let space = ui.input(|input| input.key_down(egui::Key::Space));
        let panning = response.dragged_by(PointerButton::Middle)
            || ((space || pan_tool) && response.dragged_by(PointerButton::Primary));
        if panning {
            let delta = response.drag_delta() * ppp;
            self.center[0] -= delta.x / self.zoom;
            self.center[1] -= delta.y / self.zoom;
        }

        // Zoom: wheel or pinch, keeping the point under the pointer fixed.
        if response.hovered() {
            let (scroll, pinch) =
                ui.input(|input| (input.smooth_scroll_delta.y, input.zoom_delta()));
            let factor = (scroll * 0.0025).exp() * pinch;
            if (factor - 1.0).abs() > f32::EPSILON
                && let Some(pointer) = response.hover_pos()
            {
                let old = self.zoom;
                let new = (old * factor).clamp(MIN_ZOOM, MAX_ZOOM);
                let from_center = (pointer - rect.center()) * ppp;
                // Document point under the pointer stays under it.
                self.center[0] += from_center.x / old - from_center.x / new;
                self.center[1] += from_center.y / old - from_center.y / new;
                self.zoom = new;
            }
        }

        let center_px = rect.center() * ppp;
        let origin = [
            (center_px.x - self.center[0] * self.zoom).round(),
            (center_px.y - self.center[1] * self.zoom).round(),
        ];
        let viewport = [rect.min.x * ppp, rect.min.y * ppp, size_px[0], size_px[1]];
        self.paint(ui, rect, origin, viewport);
    }

    fn paint(&mut self, ui: &Ui, rect: Rect, origin: [f32; 2], viewport: [f32; 4]) {
        let revision = self.bus.revision();
        if revision != self.sent {
            self.snapshot = Arc::new(self.document.clone());
        }
        let callback = CanvasCallback {
            document: Arc::clone(&self.snapshot),
            revision,
            base: self.sent,
            changes: self.bus.changes_since(self.sent).changes,
            view: CanvasView {
                origin,
                scale: self.zoom,
                viewport,
                background: rgb(palette::CANVAS),
                checker: [rgb(palette::CHECKER_LIGHT), rgb(palette::CHECKER_DARK)],
                checker_size: CHECKER_SIZE,
            },
        };
        self.sent = revision;
        ui.painter()
            .add(egui_wgpu::Callback::new_paint_callback(rect, callback));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cygnus_photo_core::Depth;
    use cygnus_photo_core::commands::DocumentCreate;

    fn session() -> Session {
        Session::from_commands(
            vec![PhotoCommand::DocumentCreate(DocumentCreate {
                width: 800,
                height: 600,
                resolution: 72.0,
                depth: Depth::U8,
                profile: Default::default(),
            })],
            Source::Test,
        )
    }

    #[test]
    fn zoom_steps_are_bounded() {
        let mut session = session();
        session.actual_size();
        session.zoom_step(1);
        assert!((session.zoom() - 1.25).abs() < 1e-6);
        session.zoom_step(-100);
        assert_eq!(session.zoom(), MIN_ZOOM);
        session.zoom_step(100);
        assert_eq!(session.zoom(), MAX_ZOOM);
    }

    #[test]
    fn document_is_built_through_the_bus() {
        let session = session();
        assert_eq!(
            (session.document().width, session.document().height),
            (800, 600)
        );
        assert!(session.bus.revision() > Revision::ZERO);
    }
}

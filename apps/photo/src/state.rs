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

//! Photo app state, split into three layers.
//!
//! - [`OpenDocument`]: worker link + per-document state (title, UI).
//! - [`PhotoUiState`]: per-document interface state (selection,
//!   tool, viewport, drag…), never pixels.
//! - [`PhotoShellState`]: global shell state (workspace,
//!   dialogs, modals).
//! - [`PhotoRuntimeState`]: ephemeral non-serializable state (file
//!   pickers, filter catalog, counters).
//!
//! The engine→UI boundary ([`apply_response`]) lives here: it is the
//! only conversion point from worker responses to UI state.

use crate::layout::dock::PhotoDockTab;
use crate::ui::{
    CreateDocumentDialogState, ExportDialogState, LayerRenameState, NodeDrag, PhotoBrushSettings,
    PhotoCanvasTool, PhotoEditMode, PhotoEngineResponse, PhotoLayerInfo, PhotoLayerThumb,
    PreviewImage, StableView, TransformDrag,
};
use cygnus_ui::layout::WorkspaceState;
use cygnus_ui::utils::ReorderDragState;
use cygnus_ui::viewport::{ViewportState, ViewportTextureCache};
use photo_engine::RenderRevision;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use uuid::Uuid;

/// Preview geometry: where the document sits inside the thumbnail
/// (infinite plane: the composite extends as soon as a layer leaves the frame).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PreviewGeom {
    /// Thumbnail dimensions in pixels.
    pub thumb_size: egui::Vec2,
    /// Full-resolution composite dimensions.
    pub full_size: egui::Vec2,
    /// Top-left corner of the document in thumbnail pixels.
    pub origin: egui::Vec2,
    /// Document dimensions in pixels.
    pub doc_size: egui::Vec2,
}

impl PreviewGeom {
    /// Thumbnail→document-pixel factor (uniform thumbnail).
    pub fn thumb_to_doc(self) -> f32 {
        if self.thumb_size.x > 0.0 && self.full_size.x > 0.0 {
            self.full_size.x / self.thumb_size.x
        } else {
            1.0
        }
    }

    /// On-screen document rect from the thumbnail's on-screen rect
    /// (`None` without valid geometry).
    pub fn doc_rect(self, dest: egui::Rect) -> Option<egui::Rect> {
        if self.thumb_size.x <= 0.0 || self.doc_size.x <= 0.0 || self.doc_size.y <= 0.0 {
            return None;
        }
        let scale = dest.width() / self.thumb_size.x;
        if scale <= 0.0 {
            return None;
        }
        let min = egui::pos2(
            dest.min.x + self.origin.x * scale,
            dest.min.y + self.origin.y * scale,
        );
        let kx = if self.full_size.x > 0.0 {
            self.thumb_size.x / self.full_size.x
        } else {
            1.0
        };
        let size = egui::vec2(self.doc_size.x * kx * scale, self.doc_size.y * kx * scale);
        Some(egui::Rect::from_min_size(min, size))
    }
}

/// Uploaded layer thumbnail (panel texture cache).
pub(crate) struct CachedLayerThumb {
    /// GPU texture (alive while the layer is displayed).
    handle: egui::TextureHandle,
    /// Served appearance version (re-upload if changed).
    version: u64,
    /// Thumbnail dimensions in pixels.
    size: egui::Vec2,
}

/// Magic wand options (context bar).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WandSettings {
    /// Largest channel difference kept, 0..=255.
    pub tolerance: f32,
    /// Flood only from the clicked pixel.
    pub contiguous: bool,
}

impl Default for WandSettings {
    fn default() -> Self {
        Self {
            tolerance: 32.0,
            contiguous: true,
        }
    }
}

/// Ellipse sampling of selection outlines.
const SELECTION_SEGMENTS: usize = 96;

impl PhotoUiState {
    /// Replaces the active selection and refreshes its cached outline.
    pub fn set_selection(&mut self, selection: Option<photo_engine::Selection>) {
        self.selection_outline = selection
            .as_ref()
            .map(|selection| selection.outline(SELECTION_SEGMENTS))
            .unwrap_or_default();
        self.selection = selection;
        self.selection_preview = None;
        self.lasso_preview.clear();
        self.needs_repaint = true;
    }

    /// Thumbnail views for the Layers panel (texture id + size).
    pub fn thumb_views(&self) -> HashMap<Uuid, crate::ui::LayerThumbView> {
        self.thumb_cache
            .iter()
            .map(|(id, cached)| {
                (
                    *id,
                    crate::ui::LayerThumbView {
                        texture_id: cached.handle.id(),
                        size: cached.size,
                    },
                )
            })
            .collect()
    }
}

/// UI state of an open document.
#[derive(Default)]
pub struct PhotoUiState {
    /// Snapshot for the panel (top of stack first).
    pub layers: Vec<PhotoLayerInfo>,
    /// Selected layer (pure UI state).
    pub selected: Option<Uuid>,
    /// Focused attachment (filter/mask, secondary selection:
    /// the owner stays the primary selection).
    pub focused_attachment: Option<crate::ui::AttachmentRef>,
    /// Active canvas tool.
    pub tool: PhotoCanvasTool,
    /// Edit mode (Vector / Pixel / Layout, 2nd top row).
    pub edit_mode: PhotoEditMode,
    /// Canvas zoom/pan.
    pub viewport: ViewportState,
    /// Engine composite texture.
    pub texture_cache: ViewportTextureCache,
    /// Last received preview (eyedropper sampling).
    pub last_preview: Option<PreviewImage>,
    /// Last preview geometry (document frame inside the thumbnail).
    pub preview_geom: Option<PreviewGeom>,
    /// Preview clipped to the document (View menu, default: infinite plane).
    pub preview_clip: bool,
    /// Layers panel thumbnail textures, per layer.
    pub thumb_cache: HashMap<Uuid, CachedLayerThumb>,
    /// In-progress brush stroke (image pixels).
    pub stroke: Vec<egui::Vec2>,
    /// Transient surface of the in-progress gesture (Phase 6G.3P): never to the
    /// worker before commit, drained on commit/cancel/tool switch.
    pub ink_overlay: photo_engine::interaction::InteractionOverlay,
    /// Transient stroke texture (gesture-bounded region).
    pub ink_texture: Option<egui::TextureHandle>,
    /// Cached frame geometry (persistent redraw without re-rendering).
    pub ink_frame: Option<crate::ui::InkFrameGeom>,
    /// Latest interaction metrics (report/tests, no display).
    pub ink_metrics: photo_engine::interaction::InteractionMetrics,
    /// Brush/eraser settings.
    pub brush: PhotoBrushSettings,
    /// Layers panel drag state.
    pub drag_state: ReorderDragState,
    /// Canvas transform gesture (Move tool overlay, `None` = idle).
    pub transform_drag: Option<TransformDrag>,
    /// Canvas node gesture (Node tool overlay, `None` = idle).
    pub node_drag: Option<NodeDrag>,
    /// View-stability baseline (document on-screen size anchor:
    /// a scope change never rescale the document, see
    /// `stabilize_factor`).
    pub stable_view: Option<StableView>,
    /// Last live transform send (worker throttle: at most one
    /// `SetTransform` per window, release always sends).
    pub last_transform_send: Option<std::time::Instant>,
    /// Last live node send (same throttle for `SetVectorPaths`).
    pub last_node_send: Option<std::time::Instant>,
    /// Inline layer rename (double-click).
    pub rename: LayerRenameState,
    /// History depths (undo/redo buttons).
    pub can_undo: bool,
    /// Redo available.
    pub can_redo: bool,
    /// Undoable step labels, oldest to newest
    /// (History panel, O003).
    pub history_undo: Vec<String>,
    /// Redoable step labels (O003).
    pub history_redo: Vec<String>,
    /// Status-bar message (translated at display time).
    pub status: crate::ui::status::PhotoStatus,
    /// Canvas grid.
    pub show_grid: bool,
    /// Active pixel selection (mirrored to the worker, clips strokes).
    pub selection: Option<photo_engine::Selection>,
    /// Marquee being dragged (drawn, not yet sent).
    pub selection_preview: Option<photo_engine::Selection>,
    /// Marquee shape of the selection tool.
    pub selection_kind: photo_engine::SelectionKind,
    /// Outline of `selection` in document pixels (marching ants),
    /// computed once per selection change (bitmap tracing is not free).
    pub selection_outline: Vec<Vec<(f32, f32)>>,
    /// Lasso path being drawn (document pixels).
    pub lasso_preview: Vec<(f32, f32)>,
    /// Magic wand options.
    pub wand: WandSettings,
    /// Paint bucket options (same tolerance / contiguity model).
    pub bucket: WandSettings,
    /// Gradient line being dragged (document pixels).
    pub gradient_preview: Option<((f32, f32), (f32, f32))>,
    /// Explicit repaint requested.
    pub needs_repaint: bool,
    /// TEMPORARY INSTRUMENTATION (perf diagnostics): applied worker
    /// responses (== received previews + errors/exports).
    pub responses_applied: u64,
    /// TEMPORARY INSTRUMENTATION: egui texture uploads
    /// (preview + stale thumbnails).
    pub texture_uploads: u64,
    /// INSTRUMENTATION 6G.1 (presentation diagnostics, no effect):
    /// bytes uploaded to the GPU (preview + thumbnails).
    pub texture_upload_bytes: u64,
    /// INSTRUMENTATION 6G.1: last measured presentation.
    pub last_presentation: PresentationTimings,
    /// Displayed render revision (`None` = no texture): stale
    /// responses do not re-upload.
    pub displayed_revision: Option<RenderRevision>,
    /// Last export size estimate received (O006, export dialog).
    pub export_estimate: Option<ExportEstimate>,
    /// On-screen rect of the preview texture last frame (true zoom
    /// readout: screen pixels per document pixel).
    pub last_dest: Option<egui::Rect>,
    /// Bumped whenever `last_preview` changes (histogram cache key: a
    /// deferred preview refreshes the data of the same render revision).
    pub last_preview_serial: u64,
    /// Latest full-resolution GPU frame (`None` = CPU preview shown).
    pub gpu_frame: Option<photo_engine::gpu_view::GpuFrame>,
    /// Its registration with egui's renderer (managed by the app's poll,
    /// which owns the render state).
    pub gpu_display: Option<crate::ui::gpu_display::GpuDisplay>,
    /// GPU drag proxy of the layer being moved (see `ui::drag_proxy`).
    pub drag_proxy: Option<crate::ui::drag_proxy::DragProxyState>,
}

/// Export size estimate for one (format, quality) pair (O006).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportEstimate {
    /// Format extension.
    pub ext: String,
    /// JPEG quality used.
    pub quality: u8,
    /// Estimated bytes (`None` = not estimable).
    pub bytes: Option<u64>,
}

/// Presentation timings and volumes (Phase 6G.1 §9–13): measured at
/// the caller (`apply_response`), never in widgets (cygnus-ui untouched).
/// `texture_set_us` = observable CPU cost of the upload call, NOT an
/// exact GPU latency (the transfer may finish after return).
#[derive(Debug, Default, Clone, Copy)]
pub struct PresentationTimings {
    /// Total response application (match decoding included).
    pub response_apply_us: u128,
    /// Thumbnail sync (`sync_thumbs`, Layers panel).
    pub thumb_sync_us: u128,
    /// `ColorImage` construction from the RGBA preview.
    pub colorimage_us: u128,
    /// `texture.set` call (preview upload).
    pub texture_set_us: u128,
    /// Preview application (ColorImage + upload).
    pub preview_apply_us: u128,
}

/// An open document: engine worker + UI state.
pub struct OpenDocument {
    /// Stable id (dock tab, layout persistence).
    pub id: Uuid,
    /// Dock tab title.
    pub title: String,
    /// Associated `.cygp` project path (O001: direct Save when
    /// present, Save As otherwise; set at dispatch, never read by
    /// the worker — pure UI state).
    pub project_path: Option<PathBuf>,
    /// Document UI state.
    pub ui: PhotoUiState,
    /// Commands to the worker.
    pub tx: std::sync::mpsc::Sender<crate::ui::engine_bridge::PhotoEngineCommand>,
    /// Worker responses (non-blocking poll).
    pub rx: Receiver<PhotoEngineResponse>,
    /// Language of the default names last sent to the worker (resent
    /// when the interface language changes).
    pub names_language: Option<cygnus_i18n::Language>,
    // NOTE: the worker JoinHandle is not kept: the thread
    // exits on its own when the `Sender` is dropped (`recv` ends).
}

/// Global shell state (single window, all documents).
pub struct PhotoShellState {
    /// Region and panel layout (persistable, see
    /// `crate::persistence`).
    pub workspace: WorkspaceState,
    /// Dockable tiles (tools, canvas, inspector, layers).
    /// `Tree` does not implement `Default`: see the manual
    /// `impl` below (empty tree, rebuilt by the app around
    /// the first document).
    pub tree: egui_tiles::Tree<PhotoDockTab>,
    /// "Create document" window (format, dimensions, orientation).
    pub new_doc_dialog: CreateDocumentDialogState,
    /// "Export" window (folder, format, validation).
    pub export_dialog: ExportDialogState,
    /// Help window visible.
    pub help_open: bool,
}

impl Default for PhotoShellState {
    /// Default shell: standard workspace + empty tree
    /// (`Tree` has no `Default`). `PhotoApp::new`
    /// immediately rebuilds the layout around the first document.
    fn default() -> Self {
        Self {
            workspace: WorkspaceState::default(),
            tree: egui_tiles::Tree::empty("photo-tree"),
            new_doc_dialog: CreateDocumentDialogState::default(),
            export_dialog: ExportDialogState::default(),
            help_open: false,
        }
    }
}

/// Ephemeral runtime state (non-serializable).
pub struct PhotoRuntimeState {
    /// Filter catalog (engine registry, static).
    pub filter_types: Vec<(String, String)>,
    /// Shortcut resolver (default bindings, O002).
    pub keys: preferences::KeybindingResolver,
    /// Open file picker in flight (non-blocking).
    pub open_picker: Option<Receiver<Option<PathBuf>>>,
    /// `.cygp` project open file picker (O001, non-blocking).
    pub project_open_picker: Option<Receiver<Option<PathBuf>>>,
    /// `.cygp` "Save as" file picker (O001, non-blocking).
    pub project_save_picker: Option<Receiver<Option<PathBuf>>>,
    /// "Untitled" counter.
    pub untitled_counter: usize,
}

impl PhotoRuntimeState {
    /// Initial runtime (engine-registry filter catalog).
    pub fn new() -> Self {
        let filter_types = photo_engine::filterable_types()
            .iter()
            .map(|definition| (definition.name.clone(), definition.type_id.clone()))
            .collect();
        Self {
            filter_types,
            keys: preferences::KeybindingResolver::from_bindings(
                &preferences::KeybindingPreferences::with_defaults().bindings,
            ),
            open_picker: None,
            project_open_picker: None,
            project_save_picker: None,
            untitled_counter: 0,
        }
    }
}

impl Default for PhotoRuntimeState {
    fn default() -> Self {
        Self::new()
    }
}

/// Uploads a thumbnail (unconditional: the caller already checked the
/// guard). Shares `sync_thumbs` and the `ThumbnailUpdated` arm (same
/// upload, same counters) — the guard CHOICE stays with the caller
/// to preserve each path's historical semantics exactly.
fn upload_layer_thumb(
    ctx: &egui::Context,
    ui: &mut PhotoUiState,
    id: Uuid,
    thumb: &PhotoLayerThumb,
) {
    let handle = ctx.load_texture(
        format!("photo_layer_thumb_{id}"),
        egui::ColorImage::from_rgba_unmultiplied(
            [thumb.width as usize, thumb.height as usize],
            &thumb.rgba,
        ),
        egui::TextureOptions::LINEAR,
    );
    // TEMPORARY INSTRUMENTATION.
    ui.texture_uploads += 1;
    // INSTRUMENTATION 6G.1: thumbnail volume (§14: separate
    // the Layers panel cost from the canvas one).
    ui.texture_upload_bytes += thumb.width as u64 * thumb.height as u64 * 4;
    ui.thumb_cache.insert(
        id,
        CachedLayerThumb {
            handle,
            version: thumb.version,
            size: egui::vec2(thumb.width as f32, thumb.height as f32),
        },
    );
}

/// Thumbnail cache: re-uploads only layers whose
/// appearance changed, purges the gone ones. Shared by both
/// state arms (`LayersChanged` and `StateChanged`). Historical `!=` guard
/// UNCHANGED (including version downgrade on undo — behavior preserved
/// byte-for-byte). Recursive over nested layers AND mask piles (mask
/// grayscale previews + child thumbnails).
fn sync_thumbs(ctx: &egui::Context, ui: &mut PhotoUiState) {
    // Collect first (no borrow held on `layers`): the upload borrows
    // `ui` mutably via the shared helper. Bounded clones (48×32 max).
    let mut pending: Vec<(Uuid, PhotoLayerThumb)> = Vec::new();
    let mut live: Vec<Uuid> = Vec::new();
    fn collect(
        layers: &[crate::ui::PhotoLayerInfo],
        cache: &std::collections::HashMap<Uuid, CachedLayerThumb>,
        pending: &mut Vec<(Uuid, PhotoLayerThumb)>,
        live: &mut Vec<Uuid>,
    ) {
        for layer in layers {
            live.push(layer.id);
            if let Some(thumb) = layer.thumb.as_ref()
                && cache
                    .get(&layer.id)
                    .is_none_or(|cached| cached.version != thumb.version)
            {
                pending.push((layer.id, thumb.clone()));
            }
            collect_sub(&layer.filters, cache, pending, live);
            collect_sub(&layer.masks, cache, pending, live);
            collect(&layer.children, cache, pending, live);
        }
    }
    fn collect_sub(
        subs: &[crate::ui::features::layers::PhotoSubLayerInfo],
        cache: &std::collections::HashMap<Uuid, CachedLayerThumb>,
        pending: &mut Vec<(Uuid, PhotoLayerThumb)>,
        live: &mut Vec<Uuid>,
    ) {
        for sub in subs {
            if let Some(thumb) = sub.thumb.as_ref() {
                live.push(sub.id);
                if cache
                    .get(&sub.id)
                    .is_none_or(|cached| cached.version != thumb.version)
                {
                    pending.push((sub.id, thumb.clone()));
                }
            }
            collect_sub(&sub.masks, cache, pending, live);
        }
    }
    collect(&ui.layers, &ui.thumb_cache, &mut pending, &mut live);
    for (id, thumb) in &pending {
        upload_layer_thumb(ctx, ui, *id, thumb);
    }
    ui.thumb_cache.retain(|id, _| live.contains(id));
}

/// Applies a worker response to a document's UI state.
///
/// Only engine→UI conversion point: snapshots + composite
/// preview (texture uploaded app-side, zero regeneration on
/// zoom/pan — state-only).
pub fn apply_response(ctx: &egui::Context, ui: &mut PhotoUiState, response: PhotoEngineResponse) {
    // INSTRUMENTATION 6G.1 (no effect): total application timing.
    let t_apply = std::time::Instant::now();
    let mut presentation = PresentationTimings::default();
    match response {
        PhotoEngineResponse::SelectionChanged { selection } => {
            ui.set_selection(selection);
        }
        PhotoEngineResponse::LayersChanged {
            layers,
            preview,
            revision,
            can_undo,
            can_redo,
            undo_labels,
            redo_labels,
            gpu_frame,
        } => {
            ui.layers = layers;
            ui.can_undo = can_undo;
            ui.can_redo = can_redo;
            ui.history_undo = undo_labels;
            ui.history_redo = redo_labels;
            // TEMPORARY INSTRUMENTATION.
            ui.responses_applied += 1;
            // Recursive lookup: nested selections survive worker
            // responses (a top-level-only check used to clear them).
            if ui.selected.is_some_and(|id| {
                crate::ui::features::layers::find_layer_in(&ui.layers, id).is_none()
            }) {
                ui.selected = None;
            }
            // Orphaned focus (owner gone): dropped too. Owners may be
            // layers or live filters (filter-owned masks).
            if ui.focused_attachment.is_some_and(|focus| {
                crate::ui::features::layers::find_layer_in(&ui.layers, focus.owner).is_none()
                    && crate::ui::features::layers::find_filter_in(&ui.layers, focus.owner)
                        .is_none()
            }) {
                ui.focused_attachment = None;
            }
            let t_thumbs = std::time::Instant::now();
            sync_thumbs(ctx, ui);
            presentation.thumb_sync_us = t_thumbs.elapsed().as_micros();
            match preview {
                Some(image) => {
                    // Anti-staleness guard: a late response
                    // (revision <= displayed texture) does not re-upload.
                    let fresh = ui
                        .displayed_revision
                        .is_none_or(|shown| revision.is_newer_than(shown));
                    if fresh {
                        // GPU frame: displayed at full resolution (its own
                        // geometry, no CPU upload); the CPU preview stays
                        // for the histogram and the eyedropper.
                        ui.preview_geom = Some(match &gpu_frame {
                            Some(frame) => PreviewGeom {
                                thumb_size: egui::vec2(frame.size.0 as f32, frame.size.1 as f32),
                                full_size: egui::vec2(frame.size.0 as f32, frame.size.1 as f32),
                                origin: egui::vec2(frame.origin.0, frame.origin.1),
                                doc_size: egui::vec2(
                                    frame.doc_size.0 as f32,
                                    frame.doc_size.1 as f32,
                                ),
                            },
                            None => PreviewGeom {
                                thumb_size: egui::vec2(image.width as f32, image.height as f32),
                                full_size: egui::vec2(
                                    image.full_width as f32,
                                    image.full_height as f32,
                                ),
                                origin: egui::vec2(image.origin_x, image.origin_y),
                                doc_size: egui::vec2(
                                    image.doc_width as f32,
                                    image.doc_height as f32,
                                ),
                            },
                        });
                        let t_preview = std::time::Instant::now();
                        if gpu_frame.is_none() {
                            let t_color = std::time::Instant::now();
                            let color = egui::ColorImage::from_rgba_unmultiplied(
                                [image.width as usize, image.height as usize],
                                &image.rgba,
                            );
                            presentation.colorimage_us = t_color.elapsed().as_micros();
                            let t_set = std::time::Instant::now();
                            ui.texture_cache.update(ctx, "photo_preview", color);
                            presentation.texture_set_us = t_set.elapsed().as_micros();
                            // TEMPORARY INSTRUMENTATION.
                            ui.texture_uploads += 1;
                            // INSTRUMENTATION 6G.1: uploaded volume (bytes).
                            ui.texture_upload_bytes += image.width as u64 * image.height as u64 * 4;
                        }
                        ui.gpu_frame = gpu_frame;
                        presentation.preview_apply_us = t_preview.elapsed().as_micros();
                        ui.last_preview = Some(image);
                        ui.last_preview_serial += 1;
                        ui.displayed_revision = Some(revision);
                        ui.status = crate::ui::status::PhotoStatus::None;
                        // The commit render landed: the drag proxy hands over.
                        if matches!(
                            ui.drag_proxy,
                            Some(crate::ui::drag_proxy::DragProxyState::Ending(..))
                        ) {
                            ui.drag_proxy = None;
                        }
                    }
                }
                None => match gpu_frame {
                    // GPU-only response (gesture tick): display the frame,
                    // keep the CPU preview data; a `PreviewRefreshed`
                    // follows once the worker goes quiet.
                    Some(frame) => {
                        let fresh = ui
                            .displayed_revision
                            .is_none_or(|shown| revision.is_newer_than(shown));
                        if fresh {
                            ui.preview_geom = Some(PreviewGeom {
                                thumb_size: egui::vec2(frame.size.0 as f32, frame.size.1 as f32),
                                full_size: egui::vec2(frame.size.0 as f32, frame.size.1 as f32),
                                origin: egui::vec2(frame.origin.0, frame.origin.1),
                                doc_size: egui::vec2(
                                    frame.doc_size.0 as f32,
                                    frame.doc_size.1 as f32,
                                ),
                            });
                            ui.gpu_frame = Some(frame);
                            ui.displayed_revision = Some(revision);
                            ui.status = crate::ui::status::PhotoStatus::None;
                            if matches!(
                                ui.drag_proxy,
                                Some(crate::ui::drag_proxy::DragProxyState::Ending(..))
                            ) {
                                ui.drag_proxy = None;
                            }
                        }
                    }
                    None => {
                        ui.texture_cache.clear();
                        ui.last_preview = None;
                        ui.last_preview_serial += 1;
                        ui.preview_geom = None;
                        ui.displayed_revision = None;
                        ui.gpu_frame = None;
                    }
                },
            }
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::StateChanged {
            layers,
            revision: _,
            can_undo,
            can_redo,
            undo_labels,
            redo_labels,
        } => {
            // Snapshot only: the displayed texture is kept as
            // is (no new render produced worker-side). A no-op drag
            // commit lands here: the proxy has nothing left to bridge.
            if matches!(
                ui.drag_proxy,
                Some(crate::ui::drag_proxy::DragProxyState::Ending(..))
            ) {
                ui.drag_proxy = None;
            }
            ui.layers = layers;
            ui.can_undo = can_undo;
            ui.can_redo = can_redo;
            ui.history_undo = undo_labels;
            ui.history_redo = redo_labels;
            // TEMPORARY INSTRUMENTATION.
            ui.responses_applied += 1;
            // Recursive lookup: nested selections survive worker
            // responses (a top-level-only check used to clear them).
            if ui.selected.is_some_and(|id| {
                crate::ui::features::layers::find_layer_in(&ui.layers, id).is_none()
            }) {
                ui.selected = None;
            }
            // Orphaned focus (owner gone): dropped too. Owners may be
            // layers or live filters (filter-owned masks).
            if ui.focused_attachment.is_some_and(|focus| {
                crate::ui::features::layers::find_layer_in(&ui.layers, focus.owner).is_none()
                    && crate::ui::features::layers::find_filter_in(&ui.layers, focus.owner)
                        .is_none()
            }) {
                ui.focused_attachment = None;
            }
            let t_thumbs = std::time::Instant::now();
            sync_thumbs(ctx, ui);
            presentation.thumb_sync_us = t_thumbs.elapsed().as_micros();
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::EngineError { error } => {
            ui.status = crate::ui::status::PhotoStatus::Error(error);
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::ExportDone { path } => {
            ui.status = crate::ui::status::PhotoStatus::Exported(path);
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::ProjectSaved { path } => {
            ui.status = crate::ui::status::PhotoStatus::Saved(path);
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::PreviewRefreshed { preview, revision } => {
            // Data for the histogram / eyedropper of the frame on screen
            // (an older revision is stale: a newer render is coming).
            if ui.displayed_revision == Some(revision) {
                ui.last_preview = Some(preview);
                ui.last_preview_serial += 1;
                ui.needs_repaint = true;
            }
        }
        PhotoEngineResponse::DragProxyReady {
            layer,
            below,
            image,
            above,
        } => {
            use crate::ui::drag_proxy::{DragProxy, DragProxyState};
            // Only the split still awaited (a drag ended meanwhile, or
            // moved to another layer: stale, dropped).
            if matches!(ui.drag_proxy, Some(DragProxyState::Requested(id)) if id == layer) {
                ui.drag_proxy = Some(DragProxyState::Ready(DragProxy::upload(
                    ctx, layer, below, image, above,
                )));
                ui.needs_repaint = true;
            }
        }
        PhotoEngineResponse::DragProxyUnavailable { layer } => {
            use crate::ui::drag_proxy::DragProxyState;
            if matches!(ui.drag_proxy, Some(DragProxyState::Requested(id)) if id == layer) {
                ui.drag_proxy = Some(DragProxyState::Unavailable(layer));
            }
        }
        PhotoEngineResponse::ExportSizeEstimated {
            ext,
            quality,
            bytes,
        } => {
            ui.export_estimate = Some(ExportEstimate {
                ext,
                quality,
                bytes,
            });
            ui.needs_repaint = true;
        }
        PhotoEngineResponse::ThumbnailUpdated { layer, thumb } => {
            // Phase 6G.3 §4A: secondary thumbnail, arriving after the canvas.
            // STRICT `>` guard (second barrier after the worker guard):
            // a result older than an existing display is ignored —
            // never a visual step back (completion order vs
            // send order decoupled by the thread). Unknown layer:
            // ignored without upload (no orphan in the cache).
            if ui.layers.iter().any(|l| l.id == layer) {
                let current = ui.thumb_cache.get(&layer).map(|cached| cached.version);
                if current.is_none_or(|v| thumb.version > v) {
                    if let Some(info) = ui.layers.iter_mut().find(|l| l.id == layer) {
                        info.thumb = Some(thumb.clone());
                    }
                    upload_layer_thumb(ctx, ui, layer, &thumb);
                }
            }
            ui.needs_repaint = true;
        }
    }
    presentation.response_apply_us = t_apply.elapsed().as_micros();
    ui.last_presentation = presentation;
}

/// Samples the composite color under `(x, y)` image pixels.
/// `None` out of bounds or without preview (the eyedropper then ignores the click).
pub fn sample_preview_color(preview: &PreviewImage, x: f32, y: f32) -> Option<[u8; 3]> {
    if preview.width == 0 || preview.height == 0 {
        return None;
    }
    let xi = x.round() as i64;
    let yi = y.round() as i64;
    if xi < 0 || yi < 0 || xi >= preview.width as i64 || yi >= preview.height as i64 {
        return None;
    }
    let offset = (yi as usize * preview.width as usize + xi as usize) * 4;
    preview
        .rgba
        .get(offset..offset + 3)
        .and_then(|pixel| <&[u8] as TryInto<&[u8; 3]>>::try_into(pixel).ok())
        .map(|pixel| [pixel[0], pixel[1], pixel[2]])
}

/// Perceived luminance of an RGB brush color (same Rec. 601 weights
/// as the engine): below 128 the brush hides like the eraser when
/// painting into a mask.
pub fn brush_luminance(color: [u8; 3]) -> f32 {
    0.299 * color[0] as f32 + 0.587 * color[1] as f32 + 0.114 * color[2] as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sample_preview_color_clamps() {
        let preview = PreviewImage {
            width: 2,
            height: 2,
            rgba: vec![
                10, 20, 30, 255, 40, 50, 60, 255, //
                70, 80, 90, 255, 100, 110, 120, 255,
            ],
            full_width: 2,
            full_height: 2,
            origin_x: 0.0,
            origin_y: 0.0,
            doc_width: 2,
            doc_height: 2,
        };
        assert_eq!(sample_preview_color(&preview, 0.0, 0.0), Some([10, 20, 30]));
        assert_eq!(
            sample_preview_color(&preview, 1.0, 1.0),
            Some([100, 110, 120])
        );
        assert_eq!(sample_preview_color(&preview, 5.0, 0.0), None);
        assert_eq!(sample_preview_color(&preview, -1.0, 0.0), None);
        let empty = PreviewImage {
            width: 0,
            height: 0,
            rgba: Vec::new(),
            full_width: 0,
            full_height: 0,
            origin_x: 0.0,
            origin_y: 0.0,
            doc_width: 0,
            doc_height: 0,
        };
        assert_eq!(sample_preview_color(&empty, 0.0, 0.0), None);
    }

    #[test]
    fn engine_error_sets_status() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::EngineError {
                error: photo_engine::session::EngineError::NothingToExport,
            },
        );
        assert_eq!(
            ui.status,
            crate::ui::status::PhotoStatus::Error(
                photo_engine::session::EngineError::NothingToExport
            )
        );
    }

    #[test]
    fn export_done_sets_status() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::ExportDone {
                path: PathBuf::from("/tmp/test.png"),
            },
        );
        assert_eq!(
            ui.status,
            crate::ui::status::PhotoStatus::Exported(PathBuf::from("/tmp/test.png"))
        );
    }

    #[test]
    fn drag_proxy_lifecycle_through_responses() {
        use crate::ui::drag_proxy::DragProxyState;
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        let layer = Uuid::new_v4();
        let ready = |layer| PhotoEngineResponse::DragProxyReady {
            layer,
            below: None,
            image: crate::ui::engine_bridge::ProxyLayerImage {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
                source_width: 1,
                source_height: 1,
                opacity: 100.0,
            },
            above: None,
        };
        // Unrequested split: ignored.
        apply_response(&ctx, &mut ui, ready(layer));
        assert!(ui.drag_proxy.is_none());
        ui.drag_proxy = Some(DragProxyState::Requested(layer));
        // Split for another layer: stale, ignored.
        apply_response(&ctx, &mut ui, ready(Uuid::new_v4()));
        assert!(matches!(ui.drag_proxy, Some(DragProxyState::Requested(_))));
        apply_response(&ctx, &mut ui, ready(layer));
        let Some(DragProxyState::Ready(proxy)) = ui.drag_proxy.take() else {
            panic!("proxy prêt attendu");
        };
        // Released: drawn until the commit render lands.
        ui.drag_proxy = Some(DragProxyState::Ending(
            proxy,
            photo_engine::Transform2D::default(),
        ));
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(5)));
        assert!(ui.drag_proxy.is_none(), "rendu du commit : relais terminé");
        // Refusal keeps the drag on live renders.
        ui.drag_proxy = Some(DragProxyState::Requested(layer));
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::DragProxyUnavailable { layer },
        );
        assert!(matches!(
            ui.drag_proxy,
            Some(DragProxyState::Unavailable(_))
        ));
    }

    #[test]
    fn deferred_preview_only_feeds_the_displayed_revision() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(4)));
        let serial = ui.last_preview_serial;
        let refreshed = |revision| PhotoEngineResponse::PreviewRefreshed {
            preview: PreviewImage {
                width: 1,
                height: 1,
                rgba: vec![9, 9, 9, 255],
                full_width: 1,
                full_height: 1,
                origin_x: 0.0,
                origin_y: 0.0,
                doc_width: 1,
                doc_height: 1,
            },
            revision,
        };
        // Stale revision: ignored.
        apply_response(&ctx, &mut ui, refreshed(RenderRevision(3)));
        assert_eq!(ui.last_preview_serial, serial);
        // Displayed revision: data refreshed, histogram key bumped.
        apply_response(&ctx, &mut ui, refreshed(RenderRevision(4)));
        assert_eq!(ui.last_preview_serial, serial + 1);
        assert_eq!(ui.last_preview.as_ref().map(|p| p.rgba[0]), Some(9));
    }

    #[test]
    fn export_estimate_is_stored() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::ExportSizeEstimated {
                ext: String::from("webp"),
                quality: 90,
                bytes: Some(1234),
            },
        );
        assert_eq!(
            ui.export_estimate,
            Some(ExportEstimate {
                ext: String::from("webp"),
                quality: 90,
                bytes: Some(1234),
            })
        );
    }

    #[test]
    fn project_saved_sets_status() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::ProjectSaved {
                path: PathBuf::from("/tmp/projet.cygp"),
            },
        );
        assert_eq!(
            ui.status,
            crate::ui::status::PhotoStatus::Saved(PathBuf::from("/tmp/projet.cygp"))
        );
    }

    fn preview_at_revision(revision: RenderRevision) -> PhotoEngineResponse {
        PhotoEngineResponse::LayersChanged {
            layers: Vec::new(),
            preview: Some(PreviewImage {
                width: 2,
                height: 2,
                rgba: vec![
                    10, 20, 30, 255, 40, 50, 60, 255, 70, 80, 90, 255, 100, 110, 120, 255,
                ],
                full_width: 2,
                full_height: 2,
                origin_x: 0.0,
                origin_y: 0.0,
                doc_width: 2,
                doc_height: 2,
            }),
            revision,
            can_undo: false,
            can_redo: false,
            undo_labels: Vec::new(),
            redo_labels: Vec::new(),
            gpu_frame: None,
        }
    }

    #[test]
    fn stale_preview_revision_does_not_reupload() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(2)));
        assert_eq!(ui.displayed_revision, Some(RenderRevision(2)));
        let uploads = ui.texture_uploads;
        let texture = ui.texture_cache.texture().expect("texture");
        // Late response: texture and preview kept.
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(1)));
        assert_eq!(ui.texture_uploads, uploads, "aucun re-téléversement");
        assert_eq!(ui.texture_cache.texture().expect("texture"), texture);
        assert_eq!(ui.displayed_revision, Some(RenderRevision(2)));
        // Newer revision: normal upload.
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(3)));
        assert_eq!(ui.texture_uploads, uploads + 1);
        assert_eq!(ui.displayed_revision, Some(RenderRevision(3)));
    }

    #[test]
    fn state_changed_keeps_displayed_texture() {
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(4)));
        let uploads = ui.texture_uploads;
        let texture = ui.texture_cache.texture().expect("texture");
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::StateChanged {
                layers: Vec::new(),
                revision: RenderRevision(4),
                can_undo: true,
                can_redo: false,
                undo_labels: vec![String::from("Opacité")],
                redo_labels: Vec::new(),
            },
        );
        // Panel up to date (undo), texture intact.
        assert!(ui.can_undo);
        assert_eq!(ui.history_undo, vec![String::from("Opacité")]);
        assert_eq!(ui.texture_uploads, uploads);
        assert_eq!(ui.texture_cache.texture().expect("texture"), texture);
        assert_eq!(ui.displayed_revision, Some(RenderRevision(4)));
    }

    #[test]
    fn upload_volumes_bytes_et_chronos() {
        // §26/§27 6G.1: each fresh upload accounts its bytes
        // (2×2×4) and timings; stale response ⇒ nothing.
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(2)));
        assert_eq!(ui.texture_uploads, 1);
        assert_eq!(ui.texture_upload_bytes, 2 * 2 * 4);
        let t = ui.last_presentation;
        assert!(
            t.response_apply_us >= t.preview_apply_us,
            "apply englobe preview"
        );
        assert!(
            t.preview_apply_us >= t.colorimage_us + t.texture_set_us,
            "preview = colorimage + texture (+ epsilon)"
        );
        // Stale: no volume, no upload timing.
        apply_response(&ctx, &mut ui, preview_at_revision(RenderRevision(1)));
        assert_eq!(ui.texture_uploads, 1);
        assert_eq!(ui.texture_upload_bytes, 2 * 2 * 4);
    }

    #[test]
    fn thumbnail_updated_applique_que_le_plus_recent() {
        // §16 6G.3 at UI level: B applied, earlier A rejected next —
        // never a visual step back, no thread or race.
        let ctx = egui::Context::default();
        let mut ui = PhotoUiState::default();
        let mut doc = photo_engine::Document::new(4, 4);
        doc.push_layer(photo_engine::LayerNode::Pixel(
            photo_engine::PixelLayer::new(
                "a",
                std::sync::Arc::new(image::DynamicImage::new_rgba8(4, 4)),
            ),
        ));
        let id = doc.root[0].id();
        ui.layers
            .push(crate::ui::PhotoLayerInfo::from_node_with_thumb(
                &doc.root[0],
                None,
            ));
        let thumb_at = |version: u64, first: u8| PhotoLayerThumb {
            width: 2,
            height: 2,
            rgba: [first, 0, 0, 255].repeat(4),
            version,
        };
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::ThumbnailUpdated {
                layer: id,
                thumb: thumb_at(12, 90),
            },
        );
        assert_eq!(ui.texture_uploads, 1);
        assert_eq!(ui.layers[0].thumb.as_ref().expect("miniature").version, 12);
        // Older generation arriving late: ignored (neither upload nor state).
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::ThumbnailUpdated {
                layer: id,
                thumb: thumb_at(11, 10),
            },
        );
        assert_eq!(ui.texture_uploads, 1, "aucun re-téléversement");
        assert_eq!(
            ui.layers[0].thumb.as_ref().expect("miniature").rgba[0],
            90,
            "B conservé, A rejeté"
        );
        // Unknown layer: ignored without panic.
        apply_response(
            &ctx,
            &mut ui,
            PhotoEngineResponse::ThumbnailUpdated {
                layer: Uuid::new_v4(),
                thumb: thumb_at(99, 1),
            },
        );
        assert_eq!(ui.texture_uploads, 1);
    }

    #[test]
    fn shell_defaults_to_usable_workspace() {
        use cygnus_ui::layout::PanelId;
        let shell = PhotoShellState::default();
        assert!(!shell.help_open);
        assert!(shell.workspace.find(PanelId::Layers).is_some());
        assert!(shell.workspace.find(PanelId::Inspector).is_some());
    }
}

#[cfg(test)]
mod geom_tests {
    use super::PreviewGeom;

    #[test]
    fn doc_rect_stable_quand_la_miniature_deborde() {
        // 28x8 thumbnail (infinite plane), 8x8 document at origin (10, 0).
        let geom = PreviewGeom {
            thumb_size: egui::vec2(28.0, 8.0),
            full_size: egui::vec2(28.0, 8.0),
            origin: egui::vec2(10.0, 0.0),
            doc_size: egui::vec2(8.0, 8.0),
        };
        assert!((geom.thumb_to_doc() - 1.0).abs() < 1e-5);
        // 280x80 screen dest at (0, 0): scale 10.
        let dest = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(280.0, 80.0));
        let rect = geom.doc_rect(dest).expect("cadre document");
        assert!((rect.min.x - 100.0).abs() < 1e-3);
        assert!((rect.min.y - 0.0).abs() < 1e-3);
        assert!((rect.width() - 80.0).abs() < 1e-3);
        assert!((rect.height() - 80.0).abs() < 1e-3);
    }

    #[test]
    fn doc_rect_none_sans_geometrie_valide() {
        let dest = egui::Rect::from_min_size(egui::pos2(0.0, 0.0), egui::vec2(100.0, 100.0));
        let empty = PreviewGeom::default();
        assert!(empty.doc_rect(dest).is_none());
    }
}

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

//! Photo Engine — Photo business logic extracted from apps/photo. PURE:
//! no UI dependency (buffers are converted app-side). Uses datatypes +
//! math-utils, exposed to apps.

pub mod command;
pub mod document;
pub mod export;
pub mod filters;
pub mod gpu;
pub mod histogram;
pub mod history;
pub mod interaction;
pub mod nodes;
pub mod paint;
pub mod presets;
pub mod project;
pub mod registry;
pub mod render_overlay;
pub mod render_pool;
pub mod renderer;
pub mod scene;
pub mod thumb_cache;
pub mod tile_key;
pub mod tiles;

pub use command::{Command, RenderEvent, RenderRevision};
pub use document::{
    AdjustmentLayer, Appearance, BlendMode, Document, FilterLayer, FilterNode, GroupLayer,
    LayerMask, LayerNode, PixelLayer, RgbaBuf, Transform2D, VectorLayer,
};
pub use export::{DEFAULT_JPEG_QUALITY, ExportFormat, estimate_export_size, export_image};
pub use filters::{filterable_types, new_filter_layer};
pub use gpu::GpuContext;
pub use histogram::Histogram;
pub use history::UndoAction;
pub use presets::{FilterPreset, find_preset, presets_for};
pub use registry::{all_definitions, definition_for};
pub use render_overlay::effect_overlay;
pub use renderer::{AppearanceStats, Renderer, WarmedAppearance, filters_signature};
pub use scene::pen::{
    PenPath, anchors, cubic_point, flatten, move_anchor, move_control, path_bounds, paths_finite,
    quad_point, to_kurbo, translate,
};
pub use scene::rasterize::{
    paths_bounds, place_vector_point, rasterize, rasterize_bounds, vector_bounds, vector_placement,
};
pub use scene::shapes::{ellipse, polygon, rect, star};
pub use scene::vello_render::render_vector_gpu;
pub use scene::{
    AdjustmentLayer as SceneAdjustmentLayer, GroupLayer as SceneGroupLayer, Layer, LayerContent,
    LayerEffect, PathSegment, RasterLayer as SceneRasterLayer, TextLayer,
    VectorLayer as SceneVectorLayer, VectorPath,
};
pub use thumb_cache::ThumbnailKey;
pub use tile_key::{
    BackendTag, CPU_KERNEL_VERSION, GPU_KERNEL_VERSION, GPU_PIXEL_THRESHOLD, TILE_FLAGS_NONE,
    TileCacheKey, TileContentSignature, grid_signature, tile_cache_key, tile_content_signature,
};

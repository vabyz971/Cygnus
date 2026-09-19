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

//! Photo Engine — logique métier Photo extraite de apps/photo
//! PUR : aucune dépendance UI (les buffers sont convertis côté app).
//! Utilise datatypes + math-utils, exposé aux apps

pub mod command;
pub mod document;
pub mod export;
pub mod filters;
pub mod gpu;
pub mod history;
pub mod nodes;
pub mod paint;
pub mod project;
pub mod registry;
pub mod render_pool;
pub mod renderer;

pub use command::{Command, RenderEvent, RenderRevision};
pub use document::{
    AdjustmentLayer, Appearance, BlendMode, Document, FilterLayer, FilterNode, GroupLayer,
    LayerMask, LayerNode, PixelLayer, RgbaBuf, Transform2D,
};
pub use export::{DEFAULT_JPEG_QUALITY, ExportFormat, export_image};
pub use filters::{filterable_types, new_filter_layer};
pub use gpu::GpuContext;
pub use history::UndoAction;
pub use registry::{all_definitions, definition_for};
pub use renderer::{AppearanceStats, Renderer, WarmedAppearance, filters_signature};

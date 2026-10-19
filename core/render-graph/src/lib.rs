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

//! Render Graph: operations derived from the Scene Graph, not its replacement.
//!
//! ```text
//! Scene Graph  ("what does the document hold?" — groups, images, …)
//!   │ derives (build / incremental sync)
//! Render Graph ("which operations produce the image?"
//!               Source → Transform → Effect → Mask → Blend → Output)
//!   │ orders (topological plan, dirty-only if requested)
//! Backend      (CPU today, wgpu tomorrow — pure trait, no textures here)
//! ```
//!
//! Rules of this crate:
//!
//! - NO `egui`, UI, widget, app, `wgpu`, texture or buffer
//!   dependency. Pixels are resolved by the [`Backend`] at execute time.
//! - The dependency graph ([`graph::Graph`]) is the
//!   invalidation engine: dirty propagation, revisions, topological order.
//!   This crate adds DERIVATION from the scene (bindings
//!   scene→render, partial per-scope rebuild) and the operation
//!   vocabulary.
//! - Partial rebuild: an attribute change (opacity,
//!   transform, visibility…) rebuilds NOTHING — it dirties the derived
//!   nodes. A structural change only rebuilds the SCOPE
//!   concerned (sibling group), never the whole graph. Rebuilding the graph
//!   recomputes no pixels: only dirty flags drive the work.
//!
//! # Examples
//!
//! ```
//! use render_graph::{Backend, NullBackend, RenderGraph, Scene};
//!
//! let mut scene = Scene::new();
//! let root = scene.create_node(scene::NodeKind::Group);
//! scene.create_child(root, scene::NodeKind::Image);
//!
//! let graph = RenderGraph::build(&scene, &Default::default());
//! let plan = graph.plan();
//! let mut backend = NullBackend::new();
//! graph.execute(&mut backend, &plan).expect("null backend never fails");
//! assert!(backend.executed() > 0);
//! ```

pub mod backend;
pub mod graph;
pub mod op;

#[cfg(test)]
mod tests;

use std::collections::HashMap;

pub use backend::{Backend, NullBackend};
pub use graph::{ExecuteReport, ExecutionPlan, RenderGraph, SyncReport};
pub use ids::Revision;
pub use op::{EffectKey, RenderEdge, RenderNode, RenderNodeId, RenderOp, RenderPort};
pub use scene::{Scene, SceneNodeId};

/// Effects per scene node: named effect chains (photo filters,
/// video effects…) in application order, provided by the domain engine
/// at derive/synchronize time.
///
/// The scene carries no effects (deliberately); this overlay is the
/// ONLY entry point for effects into derivation — and the
/// integration point for existing engines (see `photo-engine`,
/// `render_overlay::effect_overlay`).
pub type EffectOverlay = HashMap<SceneNodeId, Vec<EffectKey>>;

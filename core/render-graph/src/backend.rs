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

//! Execution boundary: the [`Backend`] trait and its null backend.
//!
//! Intended separation:
//!
//! ```text
//! RenderGraph ──plan──▶ Backend ──specialization──▶ wgpu (later)
//! ```
//!
//! The backend receives nodes in topological order with their typed
//! inputs and resolves contents itself (scene, pixels, caches). This
//! crate carries neither textures nor GPU handles — only ids.

use crate::{RenderEdge, RenderNode, RenderNodeId};

/// Render Graph execution backend (CPU today, GPU tomorrow).
///
/// Contract: `execute` is called with a topological plan
/// ([`crate::RenderGraph::plan`]); the backend may assume each input
/// already produced. No method allocates graph-side: `inputs` is a
/// slice over the stored edges.
pub trait Backend {
    /// Opens an execution pass (frame, target — backend semantics).
    fn begin_frame(&mut self) {}

    /// Executes ONE node. `inputs` lists `(from, port)` in any
    /// order — the backend tells background/foreground apart via the port.
    ///
    /// # Errors
    ///
    /// Returns a message on execution failure (the graph propagates
    /// the error with no observable partial state).
    fn execute_node(&mut self, node: &RenderNode, inputs: &[RenderEdge]) -> Result<(), String>;

    /// Closes the pass (presentation — backend semantics).
    fn end_frame(&mut self) {}
}

/// Null backend: records the execution order, produces nothing.
///
/// Used for tests, invalidation measurements and as a template for future
/// backends (CPU pixels, then wgpu).
#[derive(Debug, Default)]
pub struct NullBackend {
    order: Vec<RenderNodeId>,
}

impl NullBackend {
    /// New null backend.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Executed nodes, in order.
    #[must_use]
    pub fn order(&self) -> &[RenderNodeId] {
        &self.order
    }

    /// Number of executed nodes.
    #[must_use]
    pub fn executed(&self) -> usize {
        self.order.len()
    }

    /// Resets the recording to zero (successive passes comparable).
    pub fn reset(&mut self) {
        self.order.clear();
    }
}

impl Backend for NullBackend {
    fn execute_node(&mut self, node: &RenderNode, _inputs: &[RenderEdge]) -> Result<(), String> {
        self.order.push(node.id);
        Ok(())
    }
}

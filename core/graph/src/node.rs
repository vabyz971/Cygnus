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

//! Graph building blocks: [`Node`] (state), [`Edge`] (dependency) and
//! [`GraphError`] (insertion rejections).
//!
//! Neither [`Node`] nor [`Edge`] carries a domain payload: the
//! graph tracks identity, revision and dirtiness; pixels,
//! audio samples and effect parameters stay in the engines.

use ids::Revision;

use crate::NodeId;

/// Node tracked by the graph: identity + revision + dirtiness.
///
/// `Copy` (no allocation): state snapshots are taken by value,
/// with no structure clones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Node {
    /// Stable identifier.
    pub id: NodeId,
    /// Last revision (creation, incident edge, dirtying).
    pub revision: Revision,
    /// True if the node (or an upstream) changed since its last computation.
    pub dirty: bool,
}

impl Node {
    pub(crate) fn new(id: NodeId, revision: Revision) -> Self {
        Self {
            id,
            revision,
            dirty: false,
        }
    }

    /// True if the node awaits recomputation.
    #[must_use]
    pub fn is_dirty(self) -> bool {
        self.dirty
    }
}

/// Directional dependency `from → to`: "`to` depends on `from`".
///
/// Data flows from `from` to `to`; invalidating `from` dirties `to`
/// (and transitively all downstream). `Copy`: edges compare and store
/// by value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    /// Upstream: the source of the dependency.
    pub from: NodeId,
    /// Downstream: the dependent.
    pub to: NodeId,
}

impl Edge {
    /// Declares that `to` depends on `from` (data `from → to`).
    pub fn new(from: NodeId, to: NodeId) -> Self {
        Self { from, to }
    }
}

/// Edge-insertion rejection — the graph stays unchanged on error.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    /// At least one endpoint is unknown to the graph.
    #[error("unknown node: {0}")]
    UnknownNode(NodeId),
    /// Self-dependency of a node.
    #[error("self-dependency is forbidden: {0}")]
    SelfLoop(NodeId),
    /// The edge would close a cycle (`from` already reachable from `to`).
    #[error("edge {from} -> {to} would create a cycle")]
    Cycle {
        /// Requested upstream.
        from: NodeId,
        /// Requested downstream.
        to: NodeId,
    },
}

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

//! Operation vocabulary: nodes, ports, typed edges.
//!
//! Minimalist with no payload: a [`RenderNode`] says WHAT to do
//! (opcode) and WHERE it comes from (scene node), never HOW (pixels,
//! shaders — the [`crate::Backend`]'s business).

use ids::EntityId;

use crate::SceneNodeId;

/// Stable identifier of a [`RenderNode`].
///
/// Fresh at each scope (re)build — the inter-`sync` identity of intact
/// scopes is preserved (their nodes are never touched).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct RenderNodeId(EntityId);

impl RenderNodeId {
    /// New random identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Null identifier — explicit sentinel, never assigned.
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// The underlying entity.
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Wraps an existing entity.
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }

    pub(crate) fn as_graph_id(self) -> graph::NodeId {
        graph::NodeId::from_entity(self.0)
    }

    pub(crate) fn from_graph_id(id: graph::NodeId) -> Self {
        Self(id.entity())
    }
}

impl Default for RenderNodeId {
    /// Null identifier — same value as [`RenderNodeId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for RenderNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{}", self.0.as_uuid())
    }
}

/// Named effect key, provided by the domain engine via the overlay
/// ([`crate::EffectOverlay`]): `"brightness_contrast"`, `"blur"`…
/// The Render Graph knows neither parameters nor implementation —
/// only the ordered identity, enough to derive and invalidate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct EffectKey(pub String);

impl EffectKey {
    /// New key from a domain effect name.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

/// Derived operation — the "what to do" of a [`RenderNode`].
///
/// A scene leaf derives `Source → Transform → [Effect…] → [Mask] →
/// `Blend`; a group derives a `Blend` folding its children; roots
/// fold into a chain feeding the single `Output`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderOp {
    /// Provides a leaf's content (pixels read by the backend).
    Source,
    /// Places the content (the backend reads the scene's local transform).
    Transform,
    /// Applies a named domain effect (overlay chain, in order).
    Effect(EffectKey),
    /// Applies the combined coverage of semantic masks.
    Mask,
    /// Composites the foreground over the background (ports [`RenderPort`]).
    Blend,
    /// Single final sink (presentation / export).
    Output,
}

/// Input port of an edge: `Blend` tells the (accumulated) background from the
/// foreground (layer content). Other opcodes only accept
/// [`RenderPort::Foreground`] (linear chain).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RenderPort {
    /// Main content (leaf chain, previous last top…).
    Foreground,
    /// Accumulation to cover (the `Blend`'s background).
    Background,
}

/// Typed edge `from → to` on `port`: "`to` reads `from` via `port`".
/// Exact mirror (ports included) of the dependency graph's inner edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RenderEdge {
    /// Read node (upstream).
    pub from: RenderNodeId,
    /// Reader node (downstream).
    pub to: RenderNodeId,
    /// Read port of `to`.
    pub port: RenderPort,
}

impl RenderEdge {
    /// New typed edge.
    pub fn new(from: RenderNodeId, to: RenderNodeId, port: RenderPort) -> Self {
        Self { from, to, port }
    }
}

/// Operation node: opcode + scene origin.
///
/// No stored revision (see [`graph::Graph::node_revision`], single source
/// of revisions), no pixels: the backend resolves content via
/// [`RenderNode::scene`].
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RenderNode {
    /// Stable identifier (intact scope preserved).
    pub id: RenderNodeId,
    /// Operation to execute.
    pub op: RenderOp,
    /// Origin scene node (`None` = synthetic `Output` sink).
    pub scene: Option<SceneNodeId>,
}

impl RenderNode {
    pub(crate) fn new(id: RenderNodeId, op: RenderOp, scene: Option<SceneNodeId>) -> Self {
        Self { id, op, scene }
    }
}

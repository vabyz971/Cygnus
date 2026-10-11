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

//! Stable identifier of a dependency-graph node.
//!
//! Newtype over [`EntityId`](ids::EntityId): same global uniqueness and same
//! persistent stability, but a distinct type — a [`NodeId`] cannot be
//! confused with a `SceneNodeId` nor a `datatypes::NodeId`.

use ids::EntityId;

/// Stable identifier of a [`crate::Node`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct NodeId(EntityId);

impl NodeId {
    /// New random identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Null identifier — explicit sentinel, never assigned by [`crate::Graph`].
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// The underlying entity (inter-system bridges).
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Wraps an existing entity (restore, migration).
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for NodeId {
    /// Null identifier — same value as [`NodeId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "g{}", self.0.as_uuid())
    }
}

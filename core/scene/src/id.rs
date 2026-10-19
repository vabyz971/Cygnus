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

//! Stable identifiers of scene nodes.
//!
//! [`SceneNodeId`] wraps [`EntityId`](ids::EntityId): same
//! global uniqueness and same persistent stability, but a distinct type — a
//! scene identifier cannot be confused with a `graph::NodeId`,
//! nor with `datatypes::NodeId` (`u32`, generic node registry).

use ids::EntityId;
use uuid::Uuid;

/// Stable identifier of a [`crate::SceneNode`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct SceneNodeId(EntityId);

impl SceneNodeId {
    /// New random (v4) identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Null identifier — reserved for tests and explicit sentinels,
    /// never assigned by [`crate::Scene`].
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// The underlying `Uuid` (persistence, inter-engine bridges).
    #[must_use]
    pub fn as_uuid(self) -> Uuid {
        self.0.as_uuid()
    }

    /// Rebuilt from an existing `Uuid` (project loading, migration
    /// from `photo-engine` `Uuid`s).
    #[must_use]
    pub fn from_uuid(id: Uuid) -> Self {
        Self(EntityId::from_uuid(id))
    }
}

impl Default for SceneNodeId {
    /// Null identifier — same value as [`SceneNodeId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for SceneNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "s{}", self.0.as_uuid())
    }
}

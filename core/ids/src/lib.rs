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

//! Identity and revision primitives shared by the suite.
//!
//! [`EntityId`] is the generic stable identifier (pixels, scene nodes,
//! graph nodes, clips…): `Uuid` v4, hence globally unique, persistent
//! across sessions, lightweight (`Copy`, 16 bytes), comparable and hashable.
//! Domains expose their own newtypes ([`scene::SceneNodeId`](https://docs.rs/scene),
//! `graph::NodeId`) wrapping [`EntityId`] to prevent any mixing
//! of identifiers across worlds.
//!
//! [`Revision`] is the shared monotonic counter: change detection,
//! invalidation and comparison with plain integers, no clones or caches.
//! Each graph/scene allocates its revisions from ITS OWN counter —
//! values are only comparable within the same owner.
//!
//! This crate depends on nothing but `serde` + `uuid`: pure CPU,
//! usable by `core`, `engines` and `apps` without pulling in `wgpu`, `egui`
//! or any domain engine.
//!
//! Distinct from `datatypes::NodeId` (`u32`, generic node-registry
//! index): this local index is neither globally unique nor persistent,
//! it is unsuitable as a runtime identity — both systems
//! coexist, with no possible type collision.

use uuid::Uuid;

/// Stable globally-unique entity identifier.
///
/// Wraps [`Uuid`] v4: assigned once, kept as is by
/// persistence (project formats), history snapshots and
/// inter-engine bridges.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct EntityId(Uuid);

impl EntityId {
    /// New random (v4) identifier.
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Null identifier — explicit sentinel (tests, missing values),
    /// never assigned by graphs/scenes.
    #[must_use]
    pub fn nil() -> Self {
        Self(Uuid::nil())
    }

    /// The underlying `Uuid` (persistence, inter-engine bridges).
    #[must_use]
    pub fn as_uuid(self) -> Uuid {
        self.0
    }

    /// Rebuilt from an existing `Uuid` (loading, migration from
    /// `photo-engine` `Uuid`s).
    #[must_use]
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl Default for EntityId {
    /// Null identifier — same value as [`EntityId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "e{}", self.0)
    }
}

/// Monotonic revision allocated by an owner (`Scene`, `Graph`…).
///
/// `0` ([`Revision::NONE`]) means "no revision emitted" and is
/// never assigned to a node. Values are only comparable within
/// the same owner (independent counters).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    serde::Serialize,
    serde::Deserialize,
)]
#[serde(transparent)]
pub struct Revision(pub u64);

impl Revision {
    /// No revision — initial value of owner counters.
    pub const NONE: Self = Self(0);

    /// Raw counter (persistence, diagnostics).
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    /// True if `self` is strictly newer than `other`.
    #[must_use]
    pub fn is_newer_than(self, other: Self) -> bool {
        self > other
    }
}

impl std::fmt::Display for Revision {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "r{}", self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn entites_uniques_stables_ordonnables() {
        let a = EntityId::new();
        let b = EntityId::new();
        assert_ne!(a, b);
        assert_ne!(a, EntityId::nil());
        assert_eq!(EntityId::from_uuid(a.as_uuid()), a);

        let mut set = HashSet::new();
        set.insert(a);
        set.insert(b);
        set.insert(a);
        assert_eq!(set.len(), 2);
        assert!(a == a.min(b) || b == a.min(b));
    }

    #[test]
    fn revisions_comparees_et_ordonnees() {
        assert!(Revision(2).is_newer_than(Revision(1)));
        assert!(!Revision(1).is_newer_than(Revision(1)));
        assert!(!Revision::NONE.is_newer_than(Revision::NONE));
        assert_eq!(Revision::NONE.get(), 0);
        assert_eq!(Revision::default(), Revision::NONE);
    }
}

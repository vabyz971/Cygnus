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

//! Identifiant stable d'un nœud du graphe de dépendances.
//!
//! Newtype de [`EntityId`](ids::EntityId) : même unicité globale et même
//! stabilité persistante, mais type distinct — un [`NodeId`] ne peut être
//! confondu ni avec un `SceneNodeId`, ni avec un `datatypes::NodeId`.

use ids::EntityId;

/// Identifiant stable d'un [`crate::Node`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct NodeId(EntityId);

impl NodeId {
    /// Nouvel identifiant aléatoire.
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Identifiant nul — sentinelle explicite, jamais attribué par [`crate::Graph`].
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// L'entité sous-jacente (ponts inter-systèmes).
    #[must_use]
    pub fn entity(self) -> EntityId {
        self.0
    }

    /// Enveloppe une entité existante (restauration, migration).
    #[must_use]
    pub fn from_entity(id: EntityId) -> Self {
        Self(id)
    }
}

impl Default for NodeId {
    /// Identifiant nul — même valeur que [`NodeId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "g{}", self.0.as_uuid())
    }
}

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

//! Identifiants stables des nœuds de la scène.
//!
//! [`SceneNodeId`] enveloppe [`EntityId`](ids::EntityId) : même unicité
//! globale et même stabilité persistante, mais type distinct — un
//! identifiant de scène ne peut être confondu ni avec un `graph::NodeId`,
//! ni avec le `datatypes::NodeId` (`u32`, registre nodal générique).

use ids::EntityId;
use uuid::Uuid;

/// Identifiant stable d'un [`crate::SceneNode`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct SceneNodeId(EntityId);

impl SceneNodeId {
    /// Nouvel identifiant aléatoire (v4).
    #[must_use]
    pub fn new() -> Self {
        Self(EntityId::new())
    }

    /// Identifiant nul — réservé aux tests et aux sentinelles explicites,
    /// jamais attribué par [`crate::Scene`].
    #[must_use]
    pub fn nil() -> Self {
        Self(EntityId::nil())
    }

    /// L'`Uuid` sous-jacent (persistance, ponts inter-moteurs).
    #[must_use]
    pub fn as_uuid(self) -> Uuid {
        self.0.as_uuid()
    }

    /// Reconstruit depuis un `Uuid` existant (chargement projet, migration
    /// depuis les `Uuid` de `photo-engine`).
    #[must_use]
    pub fn from_uuid(id: Uuid) -> Self {
        Self(EntityId::from_uuid(id))
    }
}

impl Default for SceneNodeId {
    /// Identifiant nul — même valeur que [`SceneNodeId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for SceneNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "s{}", self.0.as_uuid())
    }
}

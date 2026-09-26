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

//! Primitifs d'identité et de révision partagés par la suite.
//!
//! [`EntityId`] est l'identifiant stable générique (pixels, nœuds de scène,
//! nœuds de graphe, clips…) : `Uuid` v4, donc globalement unique, persistant
//! entre sessions, léger (`Copy`, 16 octets), comparable et hashable. Les
//! domaines exposent leurs propres newtypes ([`scene::SceneNodeId`](https://docs.rs/scene),
//! `graph::NodeId`) enveloppant [`EntityId`] pour empêcher tout mélange
//! d'identifiants entre mondes.
//!
//! [`Revision`] est le compteur monotone partagé : détection de changement,
//! invalidation et comparaison par simples entiers, sans clones ni caches.
//! Chaque graphe/scène alloue ses révisions depuis SON compteur — les
//! valeurs ne sont comparables qu'au sein du même propriétaire.
//!
//! Cette crate ne dépend de rien d'autre que `serde` + `uuid` : CPU pur,
//! utilisable par `core`, `engines` et `apps` sans tirer `wgpu`, `egui`
//! ni aucun moteur de domaine.
//!
//! Distinct de `datatypes::NodeId` (`u32`, index du registre nodal
//! générique) : cet index local n'est ni globalement unique ni persistant,
//! il ne convient pas comme identité runtime — les deux systèmes
//! coexistent, sans collision de type possible.

use uuid::Uuid;

/// Identifiant d'entité stable et globalement unique.
///
/// Enveloppe [`Uuid`] v4 : attribué une fois, conservé tel quel par la
/// persistance (formats projet), les snapshots d'historique et les ponts
/// inter-moteurs.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[serde(transparent)]
pub struct EntityId(Uuid);

impl EntityId {
    /// Nouvel identifiant aléatoire (v4).
    #[must_use]
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Identifiant nul — sentinelle explicite (tests, valeurs manquantes),
    /// jamais attribué par les graphes/scènes.
    #[must_use]
    pub fn nil() -> Self {
        Self(Uuid::nil())
    }

    /// L'`Uuid` sous-jacent (persistance, ponts inter-moteurs).
    #[must_use]
    pub fn as_uuid(self) -> Uuid {
        self.0
    }

    /// Reconstruit depuis un `Uuid` existant (chargement, migration depuis
    /// les `Uuid` de `photo-engine`).
    #[must_use]
    pub fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }
}

impl Default for EntityId {
    /// Identifiant nul — même valeur que [`EntityId::nil`].
    fn default() -> Self {
        Self::nil()
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "e{}", self.0)
    }
}

/// Révision monotone allouée par un propriétaire (`Scene`, `Graph`…).
///
/// `0` ([`Revision::NONE`]) signifie « aucune révision émise » et n'est
/// jamais attribuée à un nœud. Les valeurs ne sont comparables qu'au sein
/// du même propriétaire (compteurs indépendants).
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
    /// Aucune révision — valeur initiale des compteurs propriétaires.
    pub const NONE: Self = Self(0);

    /// Compteur brut (persistance, diagnostics).
    #[must_use]
    pub fn get(self) -> u64 {
        self.0
    }

    /// Vrai si `self` est strictement plus récente que `other`.
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

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

//! Briques du graphe : [`Node`] (état), [`Edge`] (dépendance) et
//! [`GraphError`] (rejets d'insertion).
//!
//! Ni [`Node`] ni [`Edge`] ne portent de charge utile de domaine : le
//! graphe suit l'identité, la révision et la saleté ; les pixels,
//! échantillons audio ou paramètres d'effets restent dans les moteurs.

use ids::Revision;

use crate::NodeId;

/// Nœud suivi par le graphe : identité + révision + saleté.
///
/// `Copy` (aucune allocation) : les instantanés d'état se font par valeur,
/// sans clones de structures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Node {
    /// Identifiant stable.
    pub id: NodeId,
    /// Dernière révision (création, arête incidente, salissure).
    pub revision: Revision,
    /// Vrai si le nœud (ou un amont) a changé depuis son dernier calcul.
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

    /// Vrai si le nœud attend un recalcul.
    #[must_use]
    pub fn is_dirty(self) -> bool {
        self.dirty
    }
}

/// Dépendance directionnelle `from → to` : « `to` dépend de `from` ».
///
/// Les données circulent de `from` vers `to` ; invalider `from` salit `to`
/// (et transitivement tout l'aval). `Copy` : les arêtes se comparent et se
/// stockent par valeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    /// Amont : la source de la dépendance.
    pub from: NodeId,
    /// Aval : le dépendant.
    pub to: NodeId,
}

impl Edge {
    /// Déclare que `to` dépend de `from` (données `from → to`).
    pub fn new(from: NodeId, to: NodeId) -> Self {
        Self { from, to }
    }
}

/// Rejet d'insertion d'arête — le graphe reste inchangé sur erreur.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum GraphError {
    /// Au moins une extrémité est inconnue du graphe.
    #[error("unknown node: {0}")]
    UnknownNode(NodeId),
    /// Dépendance d'un nœud envers lui-même.
    #[error("self-dependency is forbidden: {0}")]
    SelfLoop(NodeId),
    /// L'arête fermerait un cycle (`from` déjà atteignable depuis `to`).
    #[error("edge {from} -> {to} would create a cycle")]
    Cycle {
        /// Amont demandé.
        from: NodeId,
        /// Aval demandé.
        to: NodeId,
    },
}

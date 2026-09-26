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

//! Graphe de dépendances générique — fondation CPU pure.
//!
//! Représente `Node` + `Edge` directionnelle + `Dependency` (adjacence) +
//! [`Revision`](ids::Revision) + état dirty, sans AUCUNE logique de domaine :
//! ni filtres photo, ni effets vidéo, ni DSP audio, ni `wgpu`, ni `egui`,
//! ni passes GPU. Les charges utiles vivent HORS du graphe, dans les
//! moteurs propriétaires, et référencent les nœuds par [`NodeId`].
//!
//! Convention d'arête (unique, à retenir) : `from → to` signifie
//! « `to` dépend de `from` » — les données circulent de `from` vers `to`,
//! et invalider `from` salit `to` (propagation aval).
//!
//! ```text
//! A → B → C   : si A change, A, B et C deviennent dirty.
//! ```
//!
//! Stratégie de révision : UNE révision fraîche estampille TOUS les nœuds
//! nouvellement salis par un même [`Graph::mark_dirty`] (un changement
//! logique = un timbre). [`Graph::clear_dirty`] ne bumpe PAS (la révision
//! identifie l'état calculé — bumper invaliderait faussement les caches).
//! Les cycles sont interdits à l'insertion ([`GraphError::Cycle`]) ; les
//! doublons d'arêtes sont des no-ops idempotents.
//!
//! # Examples
//!
//! ```
//! use graph::Graph;
//!
//! let mut graph = Graph::new();
//! let a = graph.add_node();
//! let b = graph.add_node();
//! let c = graph.add_node();
//! graph.add_edge(a, b).expect("acyclic");
//! graph.add_edge(b, c).expect("acyclic");
//! graph.mark_dirty(a).expect("exists");
//! assert_eq!(graph.topo_order(), vec![a, b, c]);
//! assert!(graph.is_dirty(c).expect("exists"));
//! ```

pub mod graph;
pub mod id;
pub mod node;

#[cfg(test)]
mod tests;

pub use graph::Graph;
pub use id::NodeId;
pub use ids::Revision;
pub use node::{Edge, GraphError, Node};

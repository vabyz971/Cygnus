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

//! Generic dependency graph — pure-CPU foundation.
//!
//! Represents `Node` + directional `Edge` + `Dependency` (adjacency) +
//! [`Revision`](ids::Revision) + dirty state, with NO domain logic at all:
//! neither photo filters, nor video effects, nor audio DSP, nor `wgpu`, nor `egui`,
//! nor GPU passes. Payloads live OUTSIDE the graph, in the
//! owning engines, and reference nodes via [`NodeId`].
//!
//! Edge convention (single, worth remembering): `from → to` means
//! "`to` depends on `from`" — data flows from `from` to `to`,
//! and invalidating `from` dirties `to` (downstream propagation).
//!
//! ```text
//! A → B → C   : if A changes, A, B and C become dirty.
//! ```
//!
//! Revision strategy: ONE fresh revision stamps ALL nodes
//! newly dirtied by the same [`Graph::mark_dirty`] (one
//! logical change = one stamp). [`Graph::clear_dirty`] does NOT bump (the revision
//! identifies the computed state — bumping would falsely invalidate caches).
//! Cycles are forbidden at insertion ([`GraphError::Cycle`]);
//! edge duplicates are idempotent no-ops.
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

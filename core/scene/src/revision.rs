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

//! Scene revisions: the shared counter [`Revision`](ids::Revision).
//!
//! Strategy (deliberately simple, no cache here):
//!
//! - [`Scene`](crate::Scene) owns a monotonic counter (`tree_revision`).
//! - Each structural or attribute mutation allocates ONE revision and
//!   applies it to the touched node **and all its ancestors**: a
//!   parent's revision therefore always reflects its subtree's state.
//! - Siblings and other branches keep their revision: an
//!   observer detects a subtree change with a simple
//!   integer comparison, no traversal.
//! - Writes with no effect (same value, identical re-attachment,
//!   removal of an unknown id) allocate NOTHING and are told apart from
//!   mutations by their return value (`None` vs `Some(revision)`).
//!
//! A future render cache will use these revisions as
//! invalidation keys — it is not built here.

pub use ids::Revision;

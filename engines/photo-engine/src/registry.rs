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

//! Photo node registry — delegates to the `nodes/` folder effects. Add an
//! effect: create `nodes/my_effect.rs` then add it to `nodes::all()`.

use crate::nodes;
use datatypes::NodeDefinition;

/// All registered node definitions.
#[must_use]
pub fn all_definitions() -> Vec<NodeDefinition> {
    nodes::all().into_iter().map(|e| e.definition).collect()
}

/// Find a definition by its `type_id`.
#[must_use]
pub fn definition_for(type_id: &str) -> Option<NodeDefinition> {
    all_definitions().into_iter().find(|d| d.type_id == type_id)
}

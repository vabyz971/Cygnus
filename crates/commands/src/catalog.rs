// This file is part of Cygnus.
// Copyright (C) 2026 vabyz971
//
// Cygnus is free software: you can redistribute it and/or modify it under
// the terms of the GNU General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Cygnus is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU General Public License for more
// details.
//
// You should have received a copy of the GNU General Public License along
// with Cygnus. If not, see <https://www.gnu.org/licenses/>.

//! Catalog of commands, with the JSON schema of their parameters: the
//! source of the MCP tools (milestone J4).

use schemars::{JsonSchema, Schema};
use serde::Serialize;

/// How far a command can be relied upon by external callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stability {
    /// Public command, part of the interface offered to agents.
    Stable,
    /// Development command (tests, scripts), not exposed by default and
    /// subject to change.
    Development,
    /// Internal command, used only as the inverse of another one.
    Internal,
}

/// Description of one command of a catalog.
#[derive(Debug, Clone, Serialize)]
pub struct CommandInfo {
    /// Stable name, as serialized (for example `layer.create`).
    pub name: &'static str,
    /// One-sentence description, in English, for tool listings.
    pub description: &'static str,
    /// Stability of the command.
    pub stability: Stability,
    /// JSON schema of the parameters.
    pub params: Schema,
}

impl CommandInfo {
    /// Describes a command whose parameters have the type `P`.
    pub fn new<P: JsonSchema>(
        name: &'static str,
        description: &'static str,
        stability: Stability,
    ) -> Self {
        Self {
            name,
            description,
            stability,
            params: schemars::schema_for!(P),
        }
    }
}

/// A command type that can describe all of its commands.
pub trait Catalog {
    /// Every command of the catalog, in a stable order.
    fn commands() -> Vec<CommandInfo>;
}

/// The catalog of `C` as a JSON array, ready to be published or compared
/// with a snapshot.
pub fn catalog_json<C: Catalog>() -> serde_json::Value {
    let commands = C::commands();
    serde_json::Value::Array(
        commands
            .iter()
            .map(|info| {
                serde_json::json!({
                    "name": info.name,
                    "description": info.description,
                    "stability": info.stability,
                    "params": info.params,
                })
            })
            .collect(),
    )
}

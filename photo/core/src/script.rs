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

//! Development scripts: a versioned JSON list of commands, applied in order
//! to an empty document through the command bus (docs/decisions/0015).
//!
//! Scripts describe test cases, benchmark documents and the demonstration
//! document. They are not the document format: `.cygnus` files arrive at
//! milestone J6 and owe nothing to this format.

use cygnus_commands::{CommandBus, Source};
use serde::{Deserialize, Serialize};

use crate::commands::{CoreError, PhotoCommand};
use crate::model::Document;

/// Version of the script format this program reads.
pub const SCRIPT_VERSION: u32 = 1;

/// A development script.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Script {
    /// Format version, see [`SCRIPT_VERSION`].
    pub version: u32,
    /// Commands, applied in order.
    pub commands: Vec<PhotoCommand>,
}

/// Why a script could not be run.
#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    /// The file is not a valid script.
    #[error("invalid script: {0}")]
    Invalid(#[from] serde_json::Error),
    /// The script was written for another format version.
    #[error("script version {found} is not supported (expected {SCRIPT_VERSION})")]
    UnsupportedVersion {
        /// Version found in the script.
        found: u32,
    },
    /// A command was refused.
    #[error("command {index} ({name}) refused: {error}")]
    Command {
        /// Position of the command in the script, from 0.
        index: usize,
        /// Name of the command.
        name: &'static str,
        /// Why it was refused.
        error: CoreError,
    },
}

impl Script {
    /// Parses a script, checking its version first.
    pub fn parse(json: &str) -> Result<Self, ScriptError> {
        #[derive(Deserialize)]
        struct VersionOnly {
            version: u32,
        }
        let found = serde_json::from_str::<VersionOnly>(json)?.version;
        if found != SCRIPT_VERSION {
            return Err(ScriptError::UnsupportedVersion { found });
        }
        Ok(serde_json::from_str(json)?)
    }

    /// Applies the script to `document` through `bus`, stopping at the first
    /// refused command.
    pub fn run(
        &self,
        bus: &mut CommandBus<PhotoCommand>,
        document: &mut Document,
        source: Source,
    ) -> Result<(), ScriptError> {
        for (index, command) in self.commands.iter().enumerate() {
            let name = cygnus_commands::Command::name(command);
            bus.execute(document, command.clone(), source)
                .map_err(|error| ScriptError::Command { index, name, error })?;
        }
        Ok(())
    }

    /// Runs the script on a fresh document and bus.
    pub fn build(&self, source: Source) -> Result<Document, ScriptError> {
        let mut document = Document::default();
        let mut bus = CommandBus::new();
        self.run(&mut bus, &mut document, source)?;
        Ok(document)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_document() {
        let script = Script::parse(
            r#"{"version": 1, "commands": [
                {"command": "document.create", "params": {"width": 10, "height": 5, "depth": "u8"}},
                {"command": "layer.create", "params": {"name": "a"}}
            ]}"#,
        )
        .unwrap();
        let document = script.build(Source::Test).unwrap();
        assert_eq!((document.width, document.height), (10, 5));
        assert_eq!(document.children(document.root).len(), 1);
    }

    #[test]
    fn other_version_is_refused() {
        let error = Script::parse(r#"{"version": 2, "commands": []}"#).unwrap_err();
        assert!(matches!(
            error,
            ScriptError::UnsupportedVersion { found: 2 }
        ));
    }

    #[test]
    fn refused_command_is_located() {
        let script = Script::parse(
            r#"{"version": 1, "commands": [
                {"command": "layer.delete", "params": {"id": 7}}
            ]}"#,
        )
        .unwrap();
        let error = script.build(Source::Test).unwrap_err();
        assert!(matches!(
            error,
            ScriptError::Command {
                index: 0,
                name: "layer.delete",
                ..
            }
        ));
    }
}

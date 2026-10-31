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

//! Versioned storage of a workspace (the layout of the interface).
//!
//! A workspace file is a JSON object `{ "version": N, "layout": ... }`. The
//! version is checked before the layout is decoded, so that a file written by
//! another version is reported as such instead of as a parse error. Callers
//! fall back to their default layout on any [`LoadError`].

use std::fmt;
use std::fs;
use std::io;
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct EnvelopeRef<'a, L> {
    version: u32,
    layout: &'a L,
}

#[derive(Deserialize)]
struct Envelope<L> {
    layout: L,
}

#[derive(Deserialize)]
struct VersionOnly {
    version: u32,
}

/// Why a workspace could not be restored.
#[derive(Debug)]
pub enum LoadError {
    /// The file could not be read.
    Io(io::Error),
    /// The file is not a valid workspace.
    Invalid(serde_json::Error),
    /// The file was written with another format version.
    UnsupportedVersion {
        /// Version found in the file.
        found: u32,
        /// Version this program reads.
        expected: u32,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "cannot read the workspace: {error}"),
            Self::Invalid(error) => write!(f, "invalid workspace: {error}"),
            Self::UnsupportedVersion { found, expected } => write!(
                f,
                "workspace format version {found} is not supported (expected {expected})"
            ),
        }
    }
}

impl std::error::Error for LoadError {}

/// Why a workspace could not be saved.
#[derive(Debug)]
pub enum SaveError {
    /// The layout could not be encoded.
    Encode(serde_json::Error),
    /// The file could not be written.
    Io(io::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode(error) => write!(f, "cannot encode the workspace: {error}"),
            Self::Io(error) => write!(f, "cannot write the workspace: {error}"),
        }
    }
}

impl std::error::Error for SaveError {}

/// Encodes `layout` with its format `version`.
pub fn to_json<L: Serialize>(version: u32, layout: &L) -> Result<String, SaveError> {
    serde_json::to_string_pretty(&EnvelopeRef { version, layout }).map_err(SaveError::Encode)
}

/// Decodes a layout, refusing any format version other than `expected`.
pub fn from_json<L: DeserializeOwned>(expected: u32, json: &str) -> Result<L, LoadError> {
    let found = serde_json::from_str::<VersionOnly>(json)
        .map_err(LoadError::Invalid)?
        .version;
    if found != expected {
        return Err(LoadError::UnsupportedVersion { found, expected });
    }
    serde_json::from_str::<Envelope<L>>(json)
        .map(|envelope| envelope.layout)
        .map_err(LoadError::Invalid)
}

/// Writes the workspace to `path`, creating its directory if needed. The file
/// is written next to its destination, then renamed, so that a crash during
/// the write never leaves a truncated workspace behind.
pub fn save<L: Serialize>(path: &Path, version: u32, layout: &L) -> Result<(), SaveError> {
    let json = to_json(version, layout)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(SaveError::Io)?;
    }
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, json).map_err(SaveError::Io)?;
    fs::rename(&temporary, path).map_err(SaveError::Io)
}

/// Reads the workspace stored at `path`. Returns `Ok(None)` when there is no
/// saved workspace yet.
pub fn load<L: DeserializeOwned>(path: &Path, version: u32) -> Result<Option<L>, LoadError> {
    match fs::read_to_string(path) {
        Ok(json) => from_json(version, &json).map(Some),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(LoadError::Io(error)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Serialize, Deserialize)]
    struct Layout {
        width: f32,
        panels: Vec<String>,
    }

    fn sample() -> Layout {
        Layout {
            width: 320.0,
            panels: vec!["a".to_owned(), "b".to_owned()],
        }
    }

    #[test]
    fn json_round_trip() {
        let json = to_json(3, &sample()).unwrap();
        assert_eq!(from_json::<Layout>(3, &json).unwrap(), sample());
    }

    #[test]
    fn other_version_is_reported() {
        let json = to_json(2, &sample()).unwrap();
        let error = from_json::<Layout>(3, &json).unwrap_err();
        assert!(matches!(
            error,
            LoadError::UnsupportedVersion {
                found: 2,
                expected: 3
            }
        ));
    }

    #[test]
    fn invalid_content_is_reported() {
        for json in ["", "not json", "{}", r#"{"version": 1, "layout": 5}"#] {
            assert!(matches!(
                from_json::<Layout>(1, json),
                Err(LoadError::Invalid(_))
            ));
        }
    }

    #[test]
    fn file_round_trip_and_missing_file() {
        let dir = std::env::temp_dir().join(format!("cygnus-ui-kit-test-{}", std::process::id()));
        let path = dir.join("nested").join("workspace.json");
        assert!(load::<Layout>(&path, 1).unwrap().is_none());
        save(&path, 1, &sample()).unwrap();
        assert_eq!(load::<Layout>(&path, 1).unwrap(), Some(sample()));
        fs::remove_dir_all(&dir).unwrap();
    }
}

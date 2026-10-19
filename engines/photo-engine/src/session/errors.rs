// Cygnus — Professional open-source creative suite
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

//! Engine errors sent to the UI: structured, language-free.
//!
//! Each variant names WHAT failed (the UI translates it) and carries the
//! parameters a message needs. `detail` fields hold the technical cause
//! reported by a lower layer (codec, file system, decoder): shown as is,
//! never parsed.

/// A failed engine operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// An image file could not be opened as a layer.
    OpenImage {
        /// Technical cause.
        detail: String,
    },
    /// Horizontal / vertical mirror failed.
    Flip {
        /// Technical cause.
        detail: String,
    },
    /// 90° rotation failed.
    Rotate {
        /// Technical cause.
        detail: String,
    },
    /// Crop refused: the layer is rotated, scaled or skewed.
    CropTransformed,
    /// Crop refused: the layer does not overlap the document.
    CropOutside,
    /// Crop failed for another reason.
    Crop {
        /// Technical cause.
        detail: String,
    },
    /// Unknown live-filter type requested.
    UnknownFilter {
        /// Registry id that was asked for.
        type_id: String,
    },
    /// Unknown adjustment preset requested.
    UnknownPreset {
        /// Preset id that was asked for.
        preset: String,
    },
    /// Nothing to export (empty document).
    NothingToExport,
    /// Export failed.
    Export {
        /// Technical cause.
        detail: String,
    },
    /// Project save failed.
    SaveProject {
        /// Technical cause.
        detail: String,
    },
    /// Project written by an unsupported format version.
    UnsupportedProjectVersion {
        /// Version found in the file.
        version: u32,
    },
    /// Project load failed for another reason.
    LoadProject {
        /// Technical cause.
        detail: String,
    },
}

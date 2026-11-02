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

//! Pixel document model of Cygnus Photo and its commands.
//!
//! The model is a tree of layers (docs/decisions/0009); it only changes
//! through the [`PhotoCommand`]s of the catalog (0010). This crate has no
//! GPU dependency: the model and the commands are tested without a GPU.

pub mod adjust;
pub mod changes;
pub mod commands;
pub mod model;
pub mod pixels;
pub mod script;

pub use adjust::{Adjustment, Filter};
pub use changes::PhotoChanges;
pub use commands::{CoreError, PhotoCommand};
pub use model::{
    BlendMode, ColorProfile, Depth, Document, Isolation, MAX_NESTING, Node, NodeId, NodeKind,
    Placement, Rect, Source, SourceId,
};

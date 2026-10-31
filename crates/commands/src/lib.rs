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

//! Command bus shared by Cygnus applications: typed and serializable command
//! catalog, undo/redo history, and generation of the MCP tool definitions.
//!
//! This crate knows nothing about images. An application defines its
//! commands as one type implementing [`Command`] and [`Catalog`]; every
//! change to its document goes through a [`CommandBus`]
//! (docs/decisions/0010).

mod bus;
mod catalog;
mod command;
mod journal;

pub use bus::{BusLimits, CommandBus, Entry, GroupId, Source};
pub use catalog::{Catalog, CommandInfo, Stability, catalog_json};
pub use command::{Applied, ChangeSet, Command};
pub use journal::{ChangesSince, Revision};

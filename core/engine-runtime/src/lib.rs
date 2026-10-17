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

//! Generic engine runtime shared by every Cygnus app (photo, video,
//! audio): the parts of an engine worker that know nothing about the
//! domain.
//!
//! - [`history`]: undo/redo over PRE-mutation snapshots with labels and
//!   gesture coalescing (one entry per slider drag / transform drag).
//! - [`batch`]: latest-value-wins folding of queued commands so a busy
//!   worker never renders intermediate gesture values.
//! - [`worker`]: the background thread loop (block on the first command,
//!   drain the queue, apply one batch, answer, wake the UI).
//!
//! Pure Rust, no UI, no GPU, no domain types: an engine plugs its own
//! command / response / snapshot types through the traits. The UI side
//! only sees channels and a wake callback.

pub mod batch;
pub mod history;
pub mod worker;

pub use batch::{Coalesce, fold_batch};
pub use history::{History, HistoryStep};
pub use worker::{EngineLoop, Waker, spawn_engine};

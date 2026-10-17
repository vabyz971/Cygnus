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
#![allow(unused_imports)]

//! Non-blocking bridge to the `photo-engine` editing session.
//!
//! The worker (live document, history, commands, rendering, thumbnails)
//! lives in `photo_engine::session`; this module only re-exports it under
//! the app's historical paths. The egui loop NEVER touches the
//! `Document`: it sends [`PhotoEngineCommand`] via `Sender` and polls
//! [`PhotoEngineResponse`] via `try_recv` each frame.

pub use photo_engine::session::*;
pub use photo_engine::session::{commands, metrics, preview, responses, thumbs, worker};

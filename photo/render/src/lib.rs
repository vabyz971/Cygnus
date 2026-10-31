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

//! Rendering for Cygnus Photo: compiles the layer tree into GPU passes
//! written in WGSL, over a tiled canvas.
//!
//! There is a single rendering path, on the GPU, shared by the interactive
//! canvas, the export, the command line and the tests (docs/decisions/0012
//! and 0013). Software adapters such as lavapipe or WARP are drivers like
//! any other: this crate has no CPU fallback.

pub mod format;
pub mod gpu;

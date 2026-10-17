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

//! Histogram feature (O006): position-independent panel plotting the
//! tone distribution of the displayed composite preview.
//!
//! Read-only (no actions). The histogram is computed by the engine
//! (`photo_engine::Histogram`, sampled) from the preview the canvas
//! already shows, cached per render revision: zoom/pan and redraws
//! never recompute it (state-only). cygnus-ui-only styling, channel
//! colors from theme tokens.

pub mod panel;

pub use panel::HistogramPanel;

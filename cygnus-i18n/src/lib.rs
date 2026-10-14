// Cygnus — Suite créative professionnelle open source
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

//! Shared internationalization for the Cygnus suite.
//!
//! Stable generic keys ([`TextKey`]) + [`Catalog`] bound to a
//! [`Language`]. One language = one file (`en`, `fr`, …).
//! Per-domain catalogs (e.g. [`PhotoCatalog`] / [`PhotoTextKey`])
//! live here too — apps hold no translation tables, they only
//! consume already-translated `&str` or a key.

pub mod catalog;
pub mod en;
pub mod fr;
pub mod key;
pub mod photo;

pub use catalog::{Catalog, Language};
pub use key::TextKey;
pub use photo::{PhotoCatalog, PhotoTextKey};

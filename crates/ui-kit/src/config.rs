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

//! Location of the configuration files (see docs/decisions/0007).
//!
//! All Cygnus applications share one configuration directory, with one
//! subdirectory per application:
//!
//! - Linux: `$XDG_CONFIG_HOME/cygnus/<app>/` (usually `~/.config/cygnus/<app>/`)
//! - macOS: `~/Library/Application Support/org.Cygnus.Cygnus/<app>/`
//! - Windows: `%APPDATA%\Cygnus\Cygnus\config\<app>\`

use std::path::PathBuf;

use directories::ProjectDirs;

/// Configuration directory of the application `app` (for example `photo`),
/// or `None` when the system provides no home directory.
pub fn app_config_dir(app: &str) -> Option<PathBuf> {
    ProjectDirs::from("org", "Cygnus", "Cygnus").map(|dirs| dirs.config_dir().join(app))
}

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

//! Integration test: the headless binary starts and exits successfully.

use std::process::Command;

#[test]
fn binary_exits_with_success() {
    let status = Command::new(env!("CARGO_BIN_EXE_cygnus"))
        .status()
        .expect("failed to run the cygnus binary");
    assert!(status.success(), "unexpected exit status: {status}");
}

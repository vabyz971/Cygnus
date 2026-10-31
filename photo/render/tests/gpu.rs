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

//! GPU availability test: opens the headless device and reports the adapter.
//!
//! With `CYGNUS_REQUIRE_GPU=1` (set by the GPU jobs of the CI), a missing or
//! unsupported adapter fails the test. Without it, the test reports the
//! absence and passes, so that machines without a GPU can still run the
//! rest of the suite. It is never silently green: the outcome is printed,
//! and appended to the CI job summary when `GITHUB_STEP_SUMMARY` is set.

use std::io::Write;

use cygnus_photo_render::gpu;

fn report(line: &str) {
    println!("{line}");
    if let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY")
        && let Ok(mut file) = std::fs::OpenOptions::new().append(true).open(path)
    {
        let _ = writeln!(file, "{line}");
    }
}

#[test]
fn headless_device_opens_on_a_supported_adapter() {
    let required = std::env::var("CYGNUS_REQUIRE_GPU").is_ok_and(|value| value == "1");
    match gpu::open_headless() {
        Ok(gpu) => {
            let info = &gpu.info;
            report(&format!(
                "GPU: {} | {:?} | {:?} | driver: {} {}",
                info.name, info.backend, info.device_type, info.driver, info.driver_info
            ));
        }
        Err(error) if !required => {
            report(&format!("GPU: none ({error}); GPU tests skipped"));
        }
        Err(error) => panic!("GPU required but unavailable: {error}"),
    }
}

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

//! Golden images (docs/decisions/0013): each `tests/golden/<case>.json`
//! script is rendered and compared with `<case>.png`, a 16-bit export made
//! on the reference adapter (lavapipe) and reviewed by eye.
//!
//! `CYGNUS_UPDATE_GOLDEN=1` rewrites the references; it refuses to run on
//! another adapter than lavapipe. On a mismatch, `<case>-actual.png` and
//! `<case>-difference.png` are written to `target/golden-failures/`.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::path::{Path, PathBuf};

use common::{compare, difference, export16, setup};
use cygnus_commands::Source;
use cygnus_photo_core::script::Script;
use cygnus_photo_render::Image;

fn cases() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden");
    let mut cases: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    cases.sort();
    cases
}

fn write(image: &Image, path: &Path) {
    let mut bytes = Vec::new();
    image.write_png(&mut bytes).unwrap();
    std::fs::write(path, bytes).unwrap();
}

#[test]
fn golden_images() {
    let Some((gpu, renderer)) = setup() else {
        return;
    };
    let update = std::env::var("CYGNUS_UPDATE_GOLDEN").is_ok_and(|value| value == "1");
    if update {
        assert!(
            gpu.info.name.contains("llvmpipe"),
            "golden images are made on lavapipe, not on {}",
            gpu.info.name
        );
    }
    let failures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/golden-failures");
    let mut failed = Vec::new();
    for case in cases() {
        let name = case.file_stem().unwrap().to_string_lossy().into_owned();
        let script = Script::parse(&std::fs::read_to_string(&case).unwrap()).unwrap();
        let document = script.build(Source::Test).unwrap();
        let actual = export16(&renderer, &document);
        let reference = case.with_extension("png");
        if update {
            write(&actual, &reference);
            println!("{name}: reference written");
            continue;
        }
        let bytes = std::fs::read(&reference)
            .unwrap_or_else(|_| panic!("{name}: missing {}", reference.display()));
        let expected = Image::read_png(&bytes).unwrap();
        let comparison = compare(&expected, &actual);
        println!("{name} on {}: {comparison:?}", gpu.info.name);
        if !comparison.passes() {
            std::fs::create_dir_all(&failures).unwrap();
            write(&actual, &failures.join(format!("{name}-actual.png")));
            write(
                &difference(&expected, &actual),
                &failures.join(format!("{name}-difference.png")),
            );
            failed.push(name);
        }
    }
    assert!(failed.is_empty(), "golden images differ: {failed:?}");
}

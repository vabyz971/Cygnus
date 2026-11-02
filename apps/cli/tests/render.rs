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

//! `cygnus render`: renders a development script to a PNG with exactly the
//! pipeline of the application.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use cygnus_commands::Source;
use cygnus_photo_core::script::Script;
use cygnus_photo_render::gpu;
use cygnus_photo_render::{ExportBits, Image, Renderer};

fn cygnus(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_cygnus"))
        .args(args)
        .output()
        .expect("failed to run the cygnus binary")
}

fn golden_case(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../photo/render/tests/golden")
        .join(name)
}

fn output_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Whether GPU tests can run here; fails if `CYGNUS_REQUIRE_GPU=1`.
fn gpu_available() -> bool {
    match gpu::open_headless() {
        Ok(_) => true,
        Err(error) => {
            let required = std::env::var("CYGNUS_REQUIRE_GPU").is_ok_and(|value| value == "1");
            assert!(!required, "GPU required but unavailable: {error}");
            println!("skipped, no GPU: {error}");
            false
        }
    }
}

#[test]
fn renders_a_script_like_the_library() {
    if !gpu_available() {
        return;
    }
    let script_path = golden_case("normal-opacity.json");
    let out = output_path("normal-opacity-cli.png");
    let output = cygnus(&[
        "render",
        script_path.to_str().unwrap(),
        out.to_str().unwrap(),
    ]);
    assert!(
        output.status.success(),
        "cygnus render failed: {}",
        stderr(&output)
    );

    let written = Image::read_png(&std::fs::read(&out).unwrap()).unwrap();
    // Same adapter, same pipeline: identical to the in-process export.
    let gpu = gpu::open_headless().unwrap();
    let renderer = Renderer::new(&gpu.device, &gpu.queue).unwrap();
    let script = Script::parse(&std::fs::read_to_string(&script_path).unwrap()).unwrap();
    let document = script.build(Source::Test).unwrap();
    let expected = renderer.export(&document, ExportBits::Sixteen).unwrap();
    assert!(
        written == expected,
        "cygnus render differs from the library export"
    );
}

#[test]
fn bit_depth_follows_the_document_or_the_option() {
    if !gpu_available() {
        return;
    }
    let script_path = golden_case("transparency-8bit.json");
    let script = script_path.to_str().unwrap();
    let default = output_path("transparency-default.png");
    let sixteen = output_path("transparency-16.png");
    assert!(
        cygnus(&["render", script, default.to_str().unwrap()])
            .status
            .success()
    );
    assert!(
        cygnus(&["render", script, sixteen.to_str().unwrap(), "--bits", "16"])
            .status
            .success()
    );
    let default = Image::read_png(&std::fs::read(default).unwrap()).unwrap();
    let sixteen = Image::read_png(&std::fs::read(sixteen).unwrap()).unwrap();
    assert_eq!(
        default.bits,
        ExportBits::Eight,
        "8-bit document exports in 8 bits"
    );
    assert_eq!(sixteen.bits, ExportBits::Sixteen);
}

#[test]
fn usage_errors_are_reported_without_crash() {
    for (args, message) in [
        (&["frobnicate"][..], "unknown command"),
        (&["render"][..], "usage"),
        (&["render", "a.json", "b.png", "--bits", "12"][..], "--bits"),
    ] {
        let output = cygnus(args);
        assert_eq!(output.status.code(), Some(2), "{args:?}");
        assert!(
            stderr(&output).contains(message),
            "{args:?}: {}",
            stderr(&output)
        );
    }
}

#[test]
fn file_errors_are_reported_without_crash() {
    let missing = output_path("does-not-exist.json");
    let output = cygnus(&["render", missing.to_str().unwrap(), "out.png"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("does-not-exist.json"));

    let invalid = output_path("invalid.json");
    std::fs::write(&invalid, r#"{"version": 99, "commands": []}"#).unwrap();
    let output = cygnus(&["render", invalid.to_str().unwrap(), "out.png"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(stderr(&output).contains("version 99"));
}

#[test]
fn a_script_nested_too_deep_is_refused_with_a_clear_message() {
    // Nine nested groups: the ninth layer.create is refused by the command
    // itself, before any rendering (docs/decisions/0021).
    let mut commands = vec![
        r#"{"command": "document.create", "params": {"width": 4, "height": 4, "depth": "u8"}}"#
            .to_owned(),
    ];
    for depth in 0..9 {
        let parent = if depth == 0 {
            String::new()
        } else {
            format!(r#", "parent": {depth}"#)
        };
        commands.push(format!(
            r#"{{"command": "layer.create", "params": {{"name": "g{depth}", "kind": {{"type": "group"}}{parent}}}}}"#
        ));
    }
    let script = output_path("too-deep.json");
    std::fs::write(
        &script,
        format!(r#"{{"version": 1, "commands": [{}]}}"#, commands.join(",")),
    )
    .unwrap();
    let output = cygnus(&["render", script.to_str().unwrap(), "out.png"]);
    assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
    assert!(stderr(&output).contains("nested"), "{}", stderr(&output));
}

#[test]
fn help_is_printed() {
    let output = cygnus(&["--help"]);
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("cygnus render"));
}

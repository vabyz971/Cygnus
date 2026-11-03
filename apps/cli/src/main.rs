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

//! Headless mode of Cygnus, for tests and agents.
//!
//! `cygnus render <script.json> <output.png>` builds a document from a
//! development script (docs/decisions/0015) through the command bus and
//! exports it with the renderer of the application: there is no other
//! rendering path. Exit codes: 0 success, 1 failure, 2 usage error.

use std::cell::OnceCell;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use cygnus_commands::Source;
use cygnus_photo_core::pixels::LayerTiles;
use cygnus_photo_core::resolve::{Merge, Resolvers};
use cygnus_photo_core::script::{Script, ScriptError};
use cygnus_photo_core::{Depth, Document, NodeId};
use cygnus_photo_io::FileImporter;
use cygnus_photo_render::gpu::{self, GpuError};
use cygnus_photo_render::{ExportBits, RenderError, Renderer};

const USAGE: &str = "\
Cygnus, headless mode.

Usage:
  cygnus render <script.json> <output.png> [--bits 8|16]
      Builds the document described by a development script and exports
      it as an sRGB PNG. The bit depth defaults to the document depth.
  cygnus --help
      Shows this help.

Environment:
  CYGNUS_ADAPTER   part of the GPU adapter name to use (for example llvmpipe)";

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("{0}\n\n{USAGE}")]
    Usage(String),
    #[error("cannot read {}: {source}", path.display())]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{}: {source}", path.display())]
    Script { path: PathBuf, source: ScriptError },
    #[error(transparent)]
    Gpu(#[from] GpuError),
    #[error("rendering failed: {0}")]
    Render(#[from] RenderError),
    #[error("cannot write {}: {reason}", path.display())]
    Write { path: PathBuf, reason: String },
}

impl CliError {
    fn exit_code(&self) -> u8 {
        match self {
            Self::Usage(_) => 2,
            _ => 1,
        }
    }
}

struct RenderArgs {
    script: PathBuf,
    output: PathBuf,
    bits: Option<ExportBits>,
}

fn parse_render(args: &[String]) -> Result<RenderArgs, CliError> {
    let mut positional = Vec::new();
    let mut bits = None;
    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        if arg == "--bits" {
            bits = match iter.next().map(String::as_str) {
                Some("8") => Some(ExportBits::Eight),
                Some("16") => Some(ExportBits::Sixteen),
                _ => return Err(CliError::Usage("--bits expects 8 or 16".to_owned())),
            };
        } else {
            positional.push(arg);
        }
    }
    match positional.as_slice() {
        [script, output] => Ok(RenderArgs {
            script: PathBuf::from(script),
            output: PathBuf::from(output),
            bits,
        }),
        _ => Err(CliError::Usage(
            "usage: cygnus render <script.json> <output.png> [--bits 8|16]".to_owned(),
        )),
    }
}

/// The GPU renderer, opened on first use.
#[derive(Default)]
struct LazyRenderer {
    opened: OnceCell<(gpu::Gpu, Renderer)>,
}

impl LazyRenderer {
    fn get(&self) -> Result<&Renderer, CliError> {
        if self.opened.get().is_none() {
            let gpu = gpu::open_headless()?;
            let renderer = Renderer::new(&gpu.device, &gpu.queue)?;
            let _ = self.opened.set((gpu, renderer));
        }
        self.opened
            .get()
            .map(|(_, renderer)| renderer)
            .ok_or(CliError::Render(RenderError::Unsupported(
                "opening the GPU",
            )))
    }
}

impl Merge for LazyRenderer {
    fn merge(&self, document: &Document, upper: NodeId) -> Result<LayerTiles, String> {
        self.get()
            .map_err(|error| error.to_string())?
            .merge(document, upper)
    }
}

fn render(args: RenderArgs) -> Result<(), CliError> {
    let text = std::fs::read_to_string(&args.script).map_err(|source| CliError::Read {
        path: args.script.clone(),
        source,
    })?;
    let script_error = |source| CliError::Script {
        path: args.script.clone(),
        source,
    };
    let script = Script::parse(&text).map_err(script_error)?;

    // Resolved commands go through the single path of photo/core: files
    // are read relative to the script, merges are rendered by the renderer
    // (docs/decisions/0020). The GPU is opened on first use, so that a
    // script refused by its commands is reported as such without a GPU.
    let folder = args
        .script
        .parent()
        .map_or_else(|| Path::new(".").to_owned(), Path::to_owned);
    let importer = FileImporter::new(folder);
    let renderer = LazyRenderer::default();
    let resolvers = Resolvers {
        import: Some(&importer),
        merge: Some(&renderer),
    };
    let document = script
        .build_with(Source::Cli, &resolvers)
        .map_err(script_error)?;
    let renderer = renderer.get()?;
    let bits = args.bits.unwrap_or(match document.depth {
        Depth::U8 => ExportBits::Eight,
        Depth::U16 => ExportBits::Sixteen,
    });
    let image = renderer.export(&document, bits)?;

    let write_error = |path: &Path, reason: String| CliError::Write {
        path: path.to_owned(),
        reason,
    };
    let mut bytes = Vec::new();
    image
        .write_png(&mut bytes)
        .map_err(|error| write_error(&args.output, error.to_string()))?;
    std::fs::write(&args.output, bytes)
        .map_err(|error| write_error(&args.output, error.to_string()))
}

fn run(args: &[String]) -> Result<(), CliError> {
    match args.split_first() {
        // Without arguments, or when asked: help, and success.
        None => {
            println!("{USAGE}");
            Ok(())
        }
        Some((command, _)) if command == "--help" || command == "-h" || command == "help" => {
            println!("{USAGE}");
            Ok(())
        }
        Some((command, rest)) if command == "render" => render(parse_render(rest)?),
        Some((command, _)) => Err(CliError::Usage(format!("unknown command {command:?}"))),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("cygnus: {error}");
            ExitCode::from(error.exit_code())
        }
    }
}

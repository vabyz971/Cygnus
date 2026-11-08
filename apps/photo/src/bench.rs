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

//! In-application run of the fluidity benchmark (docs/decisions/0014),
//! enabled by `CYGNUS_BENCH=fluidity`. It plays the camera path of
//! `cygnus_photo_render::bench` in the real window, with the vertical
//! synchronization off, then writes the report and closes the window.
//! This development mode adds nothing to the interface.

use std::time::Instant;

use cygnus_commands::Source;
use cygnus_photo_render::bench::{self, CameraFrame, FrameSample, RunInfo};
use eframe::egui_wgpu;

use crate::canvas::{self, Session};

/// Environment variable enabling the benchmark.
pub const ENV: &str = "CYGNUS_BENCH";
/// Environment variable giving the report path.
pub const OUT_ENV: &str = "CYGNUS_BENCH_OUT";

/// Repetitions of a settling frame at most.
const MAX_REPEATS: u32 = 10_000;

/// Progress of the run.
pub struct BenchRun {
    path: Vec<CameraFrame>,
    next: usize,
    started: Option<(Instant, usize)>,
    samples: Vec<FrameSample>,
    viewport: [u32; 2],
    finished: bool,
    /// Repetitions of the current settling frame.
    repeats: u32,
}

impl BenchRun {
    /// Builds the benchmark document and the session showing it.
    pub fn start() -> Result<(Self, Session), String> {
        eprintln!("cygnus-photo: building the benchmark document...");
        let document = bench::build_document().map_err(|error| error.to_string())?;
        Ok((
            Self {
                path: Vec::new(),
                next: 0,
                started: None,
                samples: Vec::new(),
                viewport: [0, 0],
                finished: false,
                repeats: 0,
            },
            Session::from_document(document),
        ))
    }

    /// Called at the start of each frame, before the canvas is shown with a
    /// viewport of `viewport` pixels: records the previous frame and sets
    /// the camera of this one. Returns false once the run is over.
    pub fn frame(
        &mut self,
        session: &mut Session,
        viewport: [f32; 2],
        render_state: Option<&egui_wgpu::RenderState>,
    ) -> bool {
        if self.finished {
            return false;
        }
        let now = Instant::now();
        let (stats, sources) = render_state
            .and_then(canvas::stats)
            .map_or(Default::default(), |(stats, _, sources)| (stats, sources));
        if let Some((start, index)) = self.started.take() {
            self.samples.push(FrameSample {
                phase: self.path[index].phase,
                time: now - start,
                stats,
                sources,
                covered: render_state.is_some_and(canvas::covered),
            });
            // A settling frame is repeated while the canvas has work left.
            let pending = render_state.is_some_and(canvas::pending);
            if self.path[index].settle && pending && self.repeats < MAX_REPEATS {
                self.repeats += 1;
                self.next = index;
            } else {
                self.repeats = 0;
            }
        }
        if self.path.is_empty() {
            // The camera path depends on the canvas size, known now.
            self.viewport = [viewport[0] as u32, viewport[1] as u32];
            self.path = bench::camera_path(viewport);
        }
        let Some(frame) = self.path.get(self.next).copied() else {
            self.finished = true;
            self.write(session, render_state);
            return false;
        };
        // The drag phases hold a gesture open, as a slider does.
        if frame.preview {
            session.begin_gesture();
        } else {
            session.end_gesture();
        }
        if let Some(edit) = frame.edit.filter(|_| self.repeats == 0)
            && let Err(error) = session.execute(bench::edit_command(edit), Source::Test)
        {
            eprintln!("cygnus-photo: benchmark edit refused: {error}");
        }
        session.set_camera(frame.scale, frame.center);
        self.started = Some((now, self.next));
        self.next += 1;
        true
    }

    fn write(&self, session: &Session, render_state: Option<&egui_wgpu::RenderState>) {
        let adapter = render_state.map_or_else(String::new, |state| {
            let info = state.adapter.get_info();
            format!(
                "{} | {:?} | {} {}",
                info.name, info.backend, info.driver, info.driver_info
            )
        });
        let (capacity, source_slots) = render_state
            .and_then(canvas::stats)
            .map_or((0, (0, 0)), |(_, capacity, sources)| {
                (capacity, sources.capacity)
            });
        let info = RunInfo {
            mode: "application".to_owned(),
            timing: "interval between consecutive frames of the real window: interface, canvas update, GPU work and presentation. No-vsync is requested, but a compositor may keep vsync: a median equal to the refresh period means vsync stayed on".to_owned(),
            adapter,
            viewport: self.viewport,
            cache_bytes: u64::from(capacity) * 256 * 256 * 8,
            document_bytes: bench::document_bytes(session.document()),
            source_slots,
            layers: None,
        };
        let report = bench::report(&info, &self.samples);
        let path =
            std::env::var(OUT_ENV).unwrap_or_else(|_| "fluidity-application.json".to_owned());
        let text = serde_json::to_string_pretty(&report).unwrap_or_default() + "\n";
        match std::fs::write(&path, text) {
            Ok(()) => eprintln!("cygnus-photo: benchmark report written to {path}"),
            Err(error) => eprintln!("cygnus-photo: cannot write {path}: {error}"),
        }
    }
}

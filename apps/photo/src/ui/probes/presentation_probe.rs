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

//! TEMPORARY presentation benches (worker response -> UI state with a
//! headless egui context). Moved out of the engine session tests: they
//! exercise the app's `apply_response`, which the engine must not see.

use crate::state::{PhotoUiState, apply_response};
use photo_engine::session::{EngineWorker, PhotoEngineCommand, PhotoEngineResponse};
use photo_engine::{Document, LayerNode, PixelLayer};
use std::sync::Arc;
use uuid::Uuid;

fn bench_doc(size: u32) -> (Document, Uuid) {
    let fond = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        size,
        size,
        image::Rgba([200, 40, 40, 255]),
    ));
    let klein = size / 4;
    let carre = image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
        klein,
        klein,
        image::Rgba([40, 200, 40, 255]),
    ));
    let mut doc = Document::new(size, size);
    doc.push_layer(LayerNode::Pixel(PixelLayer::new("fond", Arc::new(fond))));
    let mut vert = PixelLayer::new("carre", Arc::new(carre));
    vert.transform.offset_x = (size / 3) as f32;
    vert.transform.offset_y = (size / 4) as f32;
    doc.push_layer(LayerNode::Pixel(vert));
    let fond_id = doc.root[0].id();
    (doc, fond_id)
}

fn stats_us(samples: &mut [u128]) -> (u128, u128, u128) {
    samples.sort_unstable();
    let min = samples[0];
    let median = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
    (min, median, p95)
}

#[test]
#[ignore]
fn bench_presentation_ui_512() {
    // Release recommended: worker paint → response → apply_response with
    // headless Context. Measures ColorImage + texture.set + thumbs + total.
    let ctx = egui::Context::default();
    let (doc, fond) = bench_doc(512);
    let mut worker = EngineWorker::new(doc);
    worker.apply(PhotoEngineCommand::PaintStroke {
        layer: fond,
        points: vec![(20.0, 20.0), (36.0, 20.0)],
        eraser: false,
        radius: 5.0,
        color: [0, 0, 255],
        opacity: 1.0,
    });
    // Replays the same response N times on fresh states (uploads + bytes).
    let mut samples = Vec::new();
    for _ in 0..7 {
        let response = worker.apply(PhotoEngineCommand::Refresh);
        let preview_bytes = match &response {
            PhotoEngineResponse::LayersChanged { preview, .. } => preview
                .as_ref()
                .map(|p| p.width as u64 * p.height as u64 * 4),
            _ => None,
        };
        let mut ui = PhotoUiState::default();
        let t = std::time::Instant::now();
        apply_response(&ctx, &mut ui, response);
        let total_us = t.elapsed().as_micros();
        let p = ui.last_presentation;
        samples.push(total_us);
        eprintln!(
            "BENCHUI apply_us={total_us} thumb_us={} colorimage_us={} texture_us={} \
             preview_us={} uploads={} upload_bytes={} preview_bytes={:?}",
            p.thumb_sync_us,
            p.colorimage_us,
            p.texture_set_us,
            p.preview_apply_us,
            ui.texture_uploads,
            ui.texture_upload_bytes,
            preview_bytes,
        );
    }
    let mut sorted = samples.clone();
    let (min, median, p95) = stats_us(&mut sorted);
    eprintln!(
        "BENCHUI op=apply_response size=512 n={} min={min} median={median} p95={p95}",
        samples.len()
    );
}

#[test]
#[ignore]
fn bench_presentation_ui_2048() {
    // 2048² UI: 3 applies (partial cold), millis per step.
    let ctx = egui::Context::default();
    let (doc, fond) = bench_doc(2048);
    let mut worker = EngineWorker::new(doc);
    for i in 0..3 {
        worker.apply(PhotoEngineCommand::PaintStroke {
            layer: fond,
            points: vec![
                (20.0 + i as f32 * 300.0, 20.0),
                (36.0 + i as f32 * 300.0, 20.0),
            ],
            eraser: false,
            radius: 5.0,
            color: [0, 0, 255],
            opacity: 1.0,
        });
        let response = worker.apply(PhotoEngineCommand::Refresh);
        let mut ui = PhotoUiState::default();
        let t = std::time::Instant::now();
        apply_response(&ctx, &mut ui, response);
        let total_us = t.elapsed().as_micros();
        let p = ui.last_presentation;
        eprintln!(
            "BENCHUI op=apply size=2048 apply_us={total_us} thumb_us={} \
             colorimage_us={} texture_us={} preview_us={} uploads={} upload_bytes={}",
            p.thumb_sync_us,
            p.colorimage_us,
            p.texture_set_us,
            p.preview_apply_us,
            ui.texture_uploads,
            ui.texture_upload_bytes,
        );
    }
}

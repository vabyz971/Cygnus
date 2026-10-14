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

//! TEMPORARY measurement probes (egui slowdown diagnostics).
//!
//! Each test measures one of 8 operations and prints durations via
//! `eprintln` (read with `cargo test -- --nocapture`). Assertions
//! only cover structural facts (no timing
//! thresholds, too fragile in CI):
//! - each mutation produces a response with a full recomposite;
//! - the appearance cache makes the 2nd composite faster (HIT);
//! - each texture `update` creates a new id (GPU realloc).
//!
//! DELETE once the report is validated and fixes are in.

use super::super::engine_bridge::{EngineWorker, PhotoEngineCommand, PhotoEngineResponse};
use image::GenericImageView;
use photo_engine::{Document, LayerNode, PixelLayer};
use std::sync::Arc;
use std::time::Instant;

/// Reference 4K dimensions (3840×2160×4 ≈ 33 MB per buffer).
const W4K: u32 = 3840;
const H4K: u32 = 2160;

fn image_4k(pixel: [u8; 4]) -> Arc<image::DynamicImage> {
    Arc::new(image::DynamicImage::ImageRgba8(
        image::RgbaImage::from_pixel(W4K, H4K, image::Rgba(pixel)),
    ))
}

/// 4K document with 3 opaque layers (bottom, middle, top).
fn doc_4k() -> Document {
    let mut doc = Document::new(W4K, H4K);
    for (name, px) in [
        ("fond", [200, 40, 40, 255]),
        ("milieu", [40, 200, 40, 255]),
        ("dessus", [40, 40, 200, 255]),
    ] {
        doc.push_layer(LayerNode::Pixel(PixelLayer::new(name, image_4k(px))));
    }
    doc
}

/// Prints the last worker command's metrics.
fn report(worker: &EngineWorker) {
    let m = worker.metrics.last.as_ref().expect("metriques");
    eprintln!(
        "op={} total={}ms mutation={}ms frame={}ms snapshot={}ms composite={}ms blend={}ms thumb={}ms geometry={}ms resolves={} fhits={} prebuilds={} threbuilds={} layers={} processed={}Mpx scope={}Mpx full={}Mpx preview={}Mpx responses={} previews={}",
        m.op,
        m.total_us as f64 / 1000.0,
        m.mutation_us as f64 / 1000.0,
        m.frame_us as f64 / 1000.0,
        m.snapshot_us as f64 / 1000.0,
        m.composite_us as f64 / 1000.0,
        m.blend_us as f64 / 1000.0,
        m.thumb_us as f64 / 1000.0,
        m.geometry_us as f64 / 1000.0,
        m.appearance_resolves,
        m.appearance_frame_hits,
        m.preview_rebuilds,
        m.thumb_rebuilds,
        m.layers_blended,
        m.pixels_processed as f64 / 1_000_000.0,
        m.scope_px as f64 / 1_000_000.0,
        m.full_px as f64 / 1_000_000.0,
        m.preview_px as f64 / 1_000_000.0,
        worker.metrics.responses,
        worker.metrics.previews,
    );
}

fn layer_id(worker: &EngineWorker, index: usize) -> uuid::Uuid {
    worker.test_document().root[index].id()
}

#[test]
fn probe_1_open_image_4k() {
    let dir = std::env::temp_dir().join(format!("cygnus-perf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("dossier");
    let path = dir.join("photo4k.png");
    // Synthetic 4K file (write measured separately).
    let t = Instant::now();
    image::save_buffer(
        &path,
        &vec![128u8; (W4K * H4K * 3) as usize],
        W4K,
        H4K,
        image::ColorType::Rgb8,
    )
    .expect("png 4k");
    eprintln!("png 4k write: {}ms", t.elapsed().as_millis());
    let mut worker = EngineWorker::new(Document::new(1, 1));
    let response = worker.apply(PhotoEngineCommand::OpenImage { path: path.clone() });
    assert!(matches!(
        response,
        PhotoEngineResponse::LayersChanged { .. }
    ));
    report(&worker);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn probe_2_toggle_visibility_4k() {
    let mut worker = EngineWorker::new(doc_4k());
    worker.apply(PhotoEngineCommand::Refresh);
    let id = layer_id(&worker, 0);
    worker.apply(PhotoEngineCommand::ToggleLayerVisibility(id));
    report(&worker);
    // Structural: the mutation produces a response with a recomposited preview.
    assert_eq!(worker.metrics.responses, 2);
    assert_eq!(worker.metrics.previews, 2);
}

#[test]
fn probe_3_opacity_slider_drag_4k() {
    let mut worker = EngineWorker::new(doc_4k());
    worker.apply(PhotoEngineCommand::Refresh);
    let id = layer_id(&worker, 0);
    let responses_before = worker.metrics.responses;
    let t = Instant::now();
    for i in 1..=20 {
        worker.apply(PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: i as f32 * 5.0,
        });
    }
    let total_ms = t.elapsed().as_millis();
    eprintln!(
        "20 ticks slider: {total_ms}ms (soit {}ms/tick)",
        total_ms / 20
    );
    report(&worker);
    // Structural: 1 recomposited response PER tick (no render coalescing).
    assert_eq!(worker.metrics.responses, responses_before + 20);
    assert_eq!(worker.metrics.previews, responses_before + 20);
}

#[test]
fn probe_4_move_layer_4k() {
    let mut worker = EngineWorker::new(doc_4k());
    worker.apply(PhotoEngineCommand::Refresh);
    let id = layer_id(&worker, 0);
    worker.apply(PhotoEngineCommand::MoveLayer {
        layer: id,
        dx: 500.0,
        dy: 200.0,
    });
    report(&worker);
    let m = worker.metrics.last.as_ref().expect("metriques");
    eprintln!("scope full MPx: {}", m.full_px as f64 / 1_000_000.0);
}

#[test]
fn probe_5_paint_stroke_4k() {
    let mut worker = EngineWorker::new(doc_4k());
    worker.apply(PhotoEngineCommand::Refresh);
    let id = layer_id(&worker, 0);
    let points: Vec<(f32, f32)> = (0..200)
        .map(|i| (i as f32 * 19.0, i as f32 * 10.0))
        .collect();
    worker.apply(PhotoEngineCommand::PaintStroke {
        layer: id,
        points,
        eraser: false,
        radius: 20.0,
        color: [0, 0, 0],
        opacity: 1.0,
    });
    report(&worker);
}

#[test]
fn probe_6_zoom_pan_state_only() {
    use cygnus_ui::viewport::ViewportState;
    let mut viewport = ViewportState::default();
    let center = egui::pos2(960.0, 540.0);
    let t = Instant::now();
    for i in 0..100_000 {
        let anchor = egui::pos2(100.0 + (i % 173) as f32, 100.0);
        viewport.zoom_by_around(1.001, anchor, center);
        viewport.pan_by(egui::vec2(1.0, -1.0));
    }
    let ns_per_op = t.elapsed().as_nanos() / 200_000;
    eprintln!("zoom/pan: {ns_per_op}ns/op (pur état, zéro traffic worker)");
    // Structural: no worker send possible (local state only).
    assert!(viewport.zoom() > 1.0);
}

#[test]
fn probe_7_add_filter_4k() {
    let mut worker = EngineWorker::new(doc_4k());
    worker.apply(PhotoEngineCommand::Refresh);
    let id = layer_id(&worker, 0);
    worker.apply(PhotoEngineCommand::AddFilter {
        layer: id,
        filter_type: String::from("brightness_contrast"),
    });
    report(&worker);
}

#[test]
fn probe_8_add_remove_mask_4k() {
    let mut worker = EngineWorker::new(doc_4k());
    worker.apply(PhotoEngineCommand::Refresh);
    let id = layer_id(&worker, 0);
    worker.apply(PhotoEngineCommand::AddMask { layer: id });
    report(&worker);
    let mask = worker.test_document().masks_of(id).expect("masques")[0].id;
    worker.apply(PhotoEngineCommand::RemoveMask { owner: id, mask });
    report(&worker);
}

#[test]
fn probe_cache_second_composite_hits() {
    let doc = doc_4k();
    let t = Instant::now();
    let first = doc.composite_preview().expect("composite");
    let cold_us = t.elapsed().as_micros();
    let t = Instant::now();
    let second = doc.composite_preview().expect("composite");
    let hot_us = t.elapsed().as_micros();
    eprintln!("composite froid: {cold_us}us, chaud: {hot_us}us");
    // Structural: the appearance cache makes the 2nd pass faster.
    assert!(hot_us < cold_us, "le cache doit accélérer le 2e composite");
    assert_eq!(first.dimensions(), second.dimensions());
}

#[test]
fn probe_texture_update_reuses_slot() {
    use cygnus_ui::viewport::ViewportTextureCache;
    let ctx = egui::Context::default();
    let mut cache = ViewportTextureCache::new();
    let image = egui::ColorImage::new([64, 64], vec![egui::Color32::RED; 64 * 64]);
    cache.update(&ctx, "probe", image.clone());
    let (id1, _) = cache.texture().expect("texture");
    cache.update(&ctx, "probe", image);
    let (id2, _) = cache.texture().expect("texture");
    eprintln!("texture id avant/après update mêmes dimensions: {id1:?} -> {id2:?}");
    // Structural: same dimensions = in-place update (stable id).
    assert_eq!(id1, id2, "update doit réutiliser le slot GPU");
    // Different dimensions = clean realloc.
    let big = egui::ColorImage::new([128, 64], vec![egui::Color32::BLUE; 128 * 64]);
    cache.update(&ctx, "probe", big);
    let (id3, size3) = cache.texture().expect("texture");
    assert_ne!(id2, id3, "nouvelles dimensions = nouvelle texture");
    assert_eq!(size3, egui::vec2(128.0, 64.0));
}

#[test]
fn probe_apply_response_upload_cost() {
    use crate::state::{PhotoUiState, apply_response};
    let ctx = egui::Context::default();
    let mut worker = EngineWorker::new(doc_4k());
    let response = worker.apply(PhotoEngineCommand::Refresh);
    let mut ui = PhotoUiState::default();
    let t = Instant::now();
    apply_response(&ctx, &mut ui, response);
    eprintln!(
        "apply_response 4K (headless, sans GPU): {}ms, uploads={}",
        t.elapsed().as_millis(),
        ui.texture_uploads
    );
    assert_eq!(ui.responses_applied, 1);
    assert!(ui.texture_uploads >= 1);
    // RGBA→ColorImage conversion vs texture update breakdown
    // (same bytes as the received preview, no real GPU in headless).
    if let Some(preview) = ui.last_preview.as_ref() {
        let t = Instant::now();
        let image = egui::ColorImage::from_rgba_unmultiplied(
            [preview.width as usize, preview.height as usize],
            &preview.rgba,
        );
        let conversion_us = t.elapsed().as_micros();
        let t = Instant::now();
        ui.texture_cache.update(&ctx, "photo_preview", image);
        let update_us = t.elapsed().as_micros();
        eprintln!("upload split: conversion={conversion_us}us set/update={update_us}us");
    }
}

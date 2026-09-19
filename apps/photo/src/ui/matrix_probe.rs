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

//! Campagne de mesures composite (matrices du rapport photo).
//!
//! Reproductibilité : `cargo test --release -p photo --bin photo matrix_
//! -- --ignored --nocapture`. Tests `#[ignore]` : trop lourds pour la CI
//! debug (composites 4K), chiffres release uniquement.
//!
//! Méthodologie par cellule `(document, opération)` :
//! - `cold` : worker neuf, première application (caches renderer froids) ;
//! - `warm` : `Refresh` sur le même worker (mêmes pixels, caches chauds).
//! Pour Undo/Redo : une mutation de setup (non mesurée) puis Undo (warm)
//! puis Redo (warm). `affected_px` est théorique (bboxes géométriques via
//! `Transform2D::doc_corners`) ; `processed`/`blend`/`layers` sont mesurés.
//! Une seule répétition par cellule en release (n=1).

use super::engine_bridge::{EngineWorker, OpMetrics, PhotoEngineCommand};
use image::GenericImageView;
use photo_engine::{Document, LayerNode, PixelLayer};
use std::sync::Arc;

const W4K: u32 = 3840;
const H4K: u32 = 2160;

type BBox = (f32, f32, f32, f32); // minx, miny, maxx, maxy, pixels document

fn bbox_area(b: BBox) -> f32 {
    ((b.2 - b.0).max(0.0)) * ((b.3 - b.1).max(0.0))
}

fn bbox_union(a: BBox, b: BBox) -> BBox {
    (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3))
}

/// Bbox d'un calque en pixels document (coins transformés).
fn layer_bbox(doc: &Document, id: uuid::Uuid) -> BBox {
    let Some(photo_engine::LayerNode::Pixel(pixels)) = doc.find(id) else {
        return (0.0, 0.0, 0.0, 0.0);
    };
    let (w, h) = pixels.source_image.dimensions();
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;
    for (x, y) in pixels.transform.doc_corners(w as f32, h as f32) {
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    (min_x, min_y, max_x, max_y)
}

/// Bbox de tous les calques en positions d'affichage `0..=top_idx`
/// (0 = dessus) : union, pour les réordonnancements.
fn union_display_range(doc: &Document, top_idx: usize) -> BBox {
    let n = doc.root.len();
    let mut acc: Option<BBox> = None;
    for display in 0..=top_idx.min(n.saturating_sub(1)) {
        let id = doc.root[n - 1 - display].id();
        let bbox = layer_bbox(doc, id);
        acc = Some(match acc {
            Some(a) => bbox_union(a, bbox),
            None => bbox,
        });
    }
    acc.unwrap_or((0.0, 0.0, 0.0, 0.0))
}

fn solid(w: u32, h: u32, rgb: [u8; 3]) -> Arc<image::DynamicImage> {
    Arc::new(image::DynamicImage::ImageRgba8(
        image::RgbaImage::from_pixel(w, h, image::Rgba([rgb[0], rgb[1], rgb[2], 255])),
    ))
}

fn push_pixel(doc: &mut Document, name: &str, image: Arc<image::DynamicImage>) -> uuid::Uuid {
    let layer = PixelLayer::new(name, image);
    let id = layer.id;
    doc.push_layer(LayerNode::Pixel(layer));
    id
}

fn doc_full(w: u32, h: u32, n: usize) -> Document {
    let palette = [
        [200, 40, 40],
        [40, 200, 40],
        [40, 40, 200],
        [200, 200, 40],
        [200, 40, 200],
        [40, 200, 200],
        [120, 120, 120],
        [200, 120, 40],
        [40, 120, 200],
        [120, 40, 200],
    ];
    let mut doc = Document::new(w, h);
    for i in 0..n {
        push_pixel(
            &mut doc,
            &format!("calque{i}"),
            solid(w, h, palette[i % 10]),
        );
    }
    doc
}

/// D1 : 2048², 3 calques pleine surface.
fn doc_d1() -> Document {
    doc_full(2048, 2048, 3)
}

/// D2 : 4K, 3 calques pleine surface.
fn doc_d2() -> Document {
    doc_full(W4K, H4K, 3)
}

/// D3 : 4K, 10 calques pleine surface.
fn doc_d3() -> Document {
    doc_full(W4K, H4K, 10)
}

/// D4 : 4K, 10 calques partiels (960×540 en grille).
fn doc_d4() -> Document {
    let mut doc = Document::new(W4K, H4K);
    for i in 0..10 {
        let id = push_pixel(
            &mut doc,
            &format!("tuile{i}"),
            solid(960, 540, [40 + i as u8 * 15, 120, 200]),
        );
        if let Some(photo_engine::LayerNode::Pixel(pixels)) = doc.find_mut(id) {
            pixels.transform.offset_x = ((i % 4) * 960) as f32;
            pixels.transform.offset_y = ((i / 4) * 540) as f32;
        }
    }
    doc
}

/// D5 : 4K, 3 calques avec offsets (un hors cadre de 500 px).
fn doc_d5() -> Document {
    let mut doc = doc_d2();
    let id = doc.root[1].id();
    if let Some(photo_engine::LayerNode::Pixel(pixels)) = doc.find_mut(id) {
        pixels.transform.offset_x = 500.0;
        pixels.transform.offset_y = 200.0;
    }
    doc
}

/// D6 : 4K, 3 calques, un masque plein sur le milieu.
fn doc_d6() -> Document {
    let mut doc = doc_d2();
    let id = doc.root[1].id();
    if let Some(masks) = doc.masks_of_mut(id) {
        masks.push(photo_engine::LayerMask::full(W4K, H4K));
    }
    doc
}

/// D7 : 4K, 3 calques, filtre brightness sur le milieu.
fn doc_d7() -> Document {
    let mut doc = doc_d2();
    let id = doc.root[1].id();
    if let Some(filter) = photo_engine::new_filter_layer("brightness_contrast") {
        doc.add_filter(id, filter);
    }
    doc
}

/// D8 : 4K, 3 calques, blend modes variés (Normal/Multiply/Screen).
fn doc_d8() -> Document {
    use photo_engine::BlendMode;
    let mut doc = doc_d2();
    let modes = [BlendMode::Normal, BlendMode::Multiply, BlendMode::Screen];
    for (layer, mode) in doc.root.iter_mut().zip(modes) {
        layer.set_blend_mode(mode);
    }
    doc
}

/// Opération adressée par index d'affichage (0 = dessus).
#[derive(Clone, Debug)]
enum MatrixOp {
    Toggle(usize),
    Opacity(usize, f32),
    Blend(usize),
    Move(usize, f32, f32),
    Reorder(usize, usize),
    Paint(usize, PaintSize),
    AddFilter(usize),
    AddMask(usize),
    RemoveMask(usize),
    Undo,
    Redo,
}

#[derive(Clone, Copy, Debug)]
enum PaintSize {
    Small,  // ~1 % du document
    Medium, // ~10 % du document
    Large,  // diagonale complète
}

fn paint_points(size: PaintSize, w: u32, h: u32) -> Vec<(f32, f32)> {
    let (w, h) = (w as f32, h as f32);
    match size {
        PaintSize::Small => (0..20)
            .map(|i| (w * 0.5 + i as f32 * 2.0, h * 0.5 + i as f32 * 2.0))
            .collect(),
        PaintSize::Medium => (0..100)
            .map(|i| (w * 0.3 + i as f32 * 8.0, h * 0.3 + i as f32 * 5.0))
            .collect(),
        PaintSize::Large => (0..200)
            .map(|i| (i as f32 * 19.0, i as f32 * 10.0))
            .collect(),
    }
}

fn stroke_bbox(points: &[(f32, f32)], radius: f32) -> BBox {
    let mut bbox = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for (x, y) in points {
        bbox.0 = bbox.0.min(x - radius);
        bbox.1 = bbox.1.min(y - radius);
        bbox.2 = bbox.2.max(x + radius);
        bbox.3 = bbox.3.max(y + radius);
    }
    bbox
}

/// Id du calque en position d'affichage `display` (0 = dessus).
fn display_id(worker: &EngineWorker, display: usize) -> uuid::Uuid {
    let doc = worker.test_document();
    let n = doc.root.len();
    doc.root[n - 1 - display.min(n.saturating_sub(1))].id()
}

fn to_command(worker: &EngineWorker, op: &MatrixOp) -> (PhotoEngineCommand, usize, f32) {
    // Retourne (commande, layers_affected_théoriques, affected_px_théoriques).
    let doc = worker.test_document();
    let n = doc.root.len();
    match op.clone() {
        MatrixOp::Toggle(i) => {
            let id = display_id(worker, i);
            let cmd = PhotoEngineCommand::ToggleLayerVisibility(id);
            (cmd, i + 1, bbox_area(layer_bbox(doc, id)))
        }
        MatrixOp::Opacity(i, v) => {
            let id = display_id(worker, i);
            let cmd = PhotoEngineCommand::SetOpacity {
                layer: id,
                opacity: v,
            };
            (cmd, i + 1, bbox_area(layer_bbox(doc, id)))
        }
        MatrixOp::Blend(i) => {
            use photo_engine::BlendMode;
            let id = display_id(worker, i);
            let cmd = PhotoEngineCommand::SetBlendMode {
                layer: id,
                mode: BlendMode::Multiply,
            };
            (cmd, i + 1, bbox_area(layer_bbox(doc, id)))
        }
        MatrixOp::Move(i, dx, dy) => {
            let id = display_id(worker, i);
            let before = layer_bbox(doc, id);
            let after = (before.0 + dx, before.1 + dy, before.2 + dx, before.3 + dy);
            let cmd = PhotoEngineCommand::MoveLayer { layer: id, dx, dy };
            (cmd, i + 1, bbox_area(bbox_union(before, after)))
        }
        MatrixOp::Reorder(from, to) => {
            let cmd = PhotoEngineCommand::ReorderLayer { from, to };
            let affected = n - from.min(to);
            // Région : union des bornes des calques dont l'ordre change.
            let region = union_display_range(doc, from.max(to));
            (cmd, affected, bbox_area(region))
        }
        MatrixOp::Paint(i, size) => {
            let id = display_id(worker, i);
            let points = paint_points(size, doc.width, doc.height);
            let bbox = stroke_bbox(&points, 20.0);
            let cmd = PhotoEngineCommand::PaintStroke {
                layer: id,
                points,
                eraser: false,
                radius: 20.0,
                color: [0, 0, 0],
                opacity: 1.0,
            };
            (cmd, i + 1, bbox_area(bbox))
        }
        MatrixOp::AddFilter(i) => {
            let id = display_id(worker, i);
            let cmd = PhotoEngineCommand::AddFilter {
                layer: id,
                filter_type: String::from("brightness_contrast"),
            };
            (cmd, i + 1, bbox_area(layer_bbox(doc, id)))
        }
        MatrixOp::AddMask(i) => {
            let id = display_id(worker, i);
            let cmd = PhotoEngineCommand::AddMask { layer: id };
            (cmd, i + 1, bbox_area(layer_bbox(doc, id)))
        }
        MatrixOp::RemoveMask(i) => {
            let id = display_id(worker, i);
            let mask = doc
                .masks_of(id)
                .and_then(|masks| masks.first().map(|m| m.id))
                .expect("masque requis pour RemoveMask");
            let cmd = PhotoEngineCommand::RemoveMask { owner: id, mask };
            (cmd, i + 1, bbox_area(layer_bbox(doc, id)))
        }
        MatrixOp::Undo => (PhotoEngineCommand::Undo, n, scope_area(doc)),
        MatrixOp::Redo => (PhotoEngineCommand::Redo, n, scope_area(doc)),
    }
}

fn scope_area(doc: &Document) -> f32 {
    (doc.width * doc.height) as f32
}

fn op_name(op: &MatrixOp) -> &'static str {
    match op {
        MatrixOp::Toggle(_) => "toggle",
        MatrixOp::Opacity(_, _) => "opacity",
        MatrixOp::Blend(_) => "blend",
        MatrixOp::Move(_, _, _) => "move",
        MatrixOp::Reorder(_, _) => "reorder",
        MatrixOp::Paint(_, PaintSize::Small) => "paint_s",
        MatrixOp::Paint(_, PaintSize::Medium) => "paint_m",
        MatrixOp::Paint(_, PaintSize::Large) => "paint_l",
        MatrixOp::AddFilter(_) => "filter",
        MatrixOp::AddMask(_) => "add_mask",
        MatrixOp::RemoveMask(_) => "remove_mask",
        MatrixOp::Undo => "undo",
        MatrixOp::Redo => "redo",
    }
}

/// Contexte d'une ligne de matrice (regroupé pour le lint args).
struct RowCtx<'a> {
    doc_id: &'a str,
    op: &'a MatrixOp,
    layer: String,
    total_layers: usize,
    affected_layers: usize,
    scope_px: u64,
    affected_px: f32,
    state: &'a str,
}

/// Imprime une ligne de matrice à partir des dernières métriques.
fn print_row(ctx: RowCtx<'_>, m: &OpMetrics) {
    let ratio = if ctx.scope_px > 0 {
        m.pixels_processed as f64 / ctx.scope_px as f64
    } else {
        0.0
    };
    eprintln!(
        "MATRIX|{}|{}|{}|{}|{}|{}|{:.0}|{}|{:.2}|{}|{}|{}|{}|{}|{}|{}|{}",
        ctx.doc_id,
        op_name(ctx.op),
        ctx.layer,
        ctx.total_layers,
        ctx.affected_layers,
        ctx.scope_px,
        ctx.affected_px,
        m.pixels_processed,
        ratio,
        (m.composite_us as f64 / 1000.0),
        (m.blend_us as f64 / 1000.0),
        (m.thumb_us as f64 / 1000.0),
        (m.total_us as f64 / 1000.0),
        m.layers_blended,
        m.preview_rebuilds,
        ctx.state,
        m.appearance_resolves,
    );
}

/// Mesure une cellule : worker neuf (cold) puis Refresh (warm).
/// Retourne le worker pour chaînage (undo/redo).
fn measure_cell(doc_id: &str, doc: Document, op: MatrixOp) -> EngineWorker {
    let total_layers = doc.root.len();
    let mut worker = EngineWorker::new(doc);
    // Cold : première application, caches froids.
    let (cmd, affected, affected_px) = to_command(&worker, &op);
    let layer = format!("{op:?}");
    worker.apply(cmd);
    let m = worker.metrics.last.clone().expect("metriques");
    print_row(
        RowCtx {
            doc_id,
            op: &op,
            layer,
            total_layers,
            affected_layers: affected,
            scope_px: m.scope_px,
            affected_px,
            state: "cold",
        },
        &m,
    );
    // Warm : mêmes pixels, caches chauds.
    worker.apply(PhotoEngineCommand::Refresh);
    let m = worker.metrics.last.clone().expect("metriques");
    let warm_scope = scope_area(worker.test_document());
    print_row(
        RowCtx {
            doc_id,
            op: &op,
            layer: String::from("refresh"),
            total_layers,
            affected_layers: total_layers,
            scope_px: m.scope_px,
            affected_px: warm_scope,
            state: "warm",
        },
        &m,
    );
    worker
}

fn ops_for_doc(doc_id: &str) -> Vec<MatrixOp> {
    let mid = 1;
    let mut ops = vec![
        MatrixOp::Toggle(mid),
        MatrixOp::Opacity(mid, 50.0),
        MatrixOp::Blend(mid),
        MatrixOp::Move(mid, 250.0, 100.0),
        MatrixOp::Reorder(0, 2),
        MatrixOp::Paint(mid, PaintSize::Medium),
        MatrixOp::AddFilter(mid),
        MatrixOp::AddMask(mid),
    ];
    if doc_id == "D6" {
        ops.push(MatrixOp::RemoveMask(mid));
    }
    ops
}

#[test]
#[ignore]
fn matrix_docs_d1_d4() {
    eprintln!(
        "MATRIX|doc|op|layer|total|affected|scope|affected_px|processed|ratio|composite_ms|blend_ms|thumb_ms|total_ms|blended|rebuilds|state|resolves"
    );
    for (id, build) in [
        ("D1", doc_d1 as fn() -> Document),
        ("D2", doc_d2 as fn() -> Document),
        ("D3", doc_d3 as fn() -> Document),
        ("D4", doc_d4 as fn() -> Document),
    ] {
        for op in ops_for_doc(id) {
            measure_cell(id, build(), op);
        }
    }
}

#[test]
#[ignore]
fn matrix_docs_d5_d8_undo_redo() {
    eprintln!(
        "MATRIX|doc|op|layer|total|affected|scope|affected_px|processed|ratio|composite_ms|blend_ms|thumb_ms|total_ms|blended|rebuilds|state|resolves"
    );
    for (id, build) in [
        ("D5", doc_d5 as fn() -> Document),
        ("D6", doc_d6 as fn() -> Document),
        ("D7", doc_d7 as fn() -> Document),
        ("D8", doc_d8 as fn() -> Document),
    ] {
        for op in ops_for_doc(id) {
            measure_cell(id, build(), op);
        }
    }
    // Undo/Redo : setup commun (mutation non mesurée), puis Undo/Redo.
    let mut worker = EngineWorker::new(doc_d2());
    let total = worker.test_document().root.len();
    let setup_id = display_id(&worker, 1);
    worker.apply(PhotoEngineCommand::SetOpacity {
        layer: setup_id,
        opacity: 50.0,
    });
    worker.apply(PhotoEngineCommand::Undo);
    let m = worker.metrics.last.clone().expect("metriques");
    print_row(
        RowCtx {
            doc_id: "D2",
            op: &MatrixOp::Undo,
            layer: String::from("undo"),
            total_layers: total,
            affected_layers: total,
            scope_px: m.scope_px,
            affected_px: scope_area(worker.test_document()),
            state: "warm",
        },
        &m,
    );
    worker.apply(PhotoEngineCommand::Redo);
    let m = worker.metrics.last.clone().expect("metriques");
    print_row(
        RowCtx {
            doc_id: "D2",
            op: &MatrixOp::Redo,
            layer: String::from("redo"),
            total_layers: total,
            affected_layers: total,
            scope_px: m.scope_px,
            affected_px: scope_area(worker.test_document()),
            state: "warm",
        },
        &m,
    );
}

#[test]
#[ignore]
fn matrix_regional_and_batch() {
    eprintln!(
        "MATRIX|doc|op|layer|total|affected|scope|affected_px|processed|ratio|composite_ms|blend_ms|thumb_ms|total_ms|blended|rebuilds|state|resolves"
    );
    // Paint : tailles de région croissantes (R1/R2/R3 ≈ 1 %/10 %/100 %).
    for size in [PaintSize::Small, PaintSize::Medium, PaintSize::Large] {
        measure_cell("D2", doc_d2(), MatrixOp::Paint(1, size));
    }
    // Move : deltas croissants (0/50/250/500 px).
    for dx in [0.0, 50.0, 250.0, 500.0] {
        if dx == 0.0 {
            continue; // no-op, couvert par ailleurs
        }
        measure_cell("D2", doc_d2(), MatrixOp::Move(1, dx, 0.0));
    }
    // Batch : 20 ticks coalescés vs 20 unitaires (D2, opacité).
    let mut worker = EngineWorker::new(doc_d2());
    let id = display_id(&worker, 1);
    let batch: Vec<PhotoEngineCommand> = (1..=20)
        .map(|i| PhotoEngineCommand::SetOpacity {
            layer: id,
            opacity: i as f32 * 4.0,
        })
        .collect();
    let total = worker.test_document().root.len();
    let t = std::time::Instant::now();
    let responses = worker.apply_batch(batch);
    let elapsed = t.elapsed().as_micros();
    let m = worker.metrics.last.clone().expect("metriques");
    eprintln!(
        "BATCH|D2|opacity20|responses={}|renders={}|total_ms={}",
        responses.len(),
        worker.metrics.renders,
        elapsed as f64 / 1000.0
    );
    print_row(
        RowCtx {
            doc_id: "D2",
            op: &MatrixOp::Opacity(1, 80.0),
            layer: String::from("batch20"),
            total_layers: total,
            affected_layers: 2,
            scope_px: m.scope_px,
            affected_px: bbox_area(layer_bbox(worker.test_document(), id)),
            state: "batch",
        },
        &m,
    );
    // Batch move : 20 pas de 25 px fusionnés (1 seul rendu).
    // Paint : pas de batch inter-strokes (1 CommitStroke = 1 réponse) → N/A.
    let mut worker = EngineWorker::new(doc_d2());
    let id = display_id(&worker, 1);
    let total = worker.test_document().root.len();
    let drag: Vec<PhotoEngineCommand> = (0..20)
        .map(|_| PhotoEngineCommand::MoveLayer {
            layer: id,
            dx: 25.0,
            dy: 0.0,
        })
        .collect();
    let t = std::time::Instant::now();
    let responses = worker.apply_batch(drag);
    let elapsed = t.elapsed().as_micros();
    let m = worker.metrics.last.clone().expect("metriques");
    eprintln!(
        "BATCH|D2|move20|responses={}|renders={}|total_ms={}",
        responses.len(),
        worker.metrics.renders,
        elapsed as f64 / 1000.0
    );
    print_row(
        RowCtx {
            doc_id: "D2",
            op: &MatrixOp::Move(1, 500.0, 0.0),
            layer: String::from("batch20"),
            total_layers: total,
            affected_layers: 2,
            scope_px: m.scope_px,
            affected_px: bbox_area(layer_bbox(worker.test_document(), id)),
            state: "batch",
        },
        &m,
    );
}

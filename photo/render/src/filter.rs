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

//! Filter stages (docs/decisions/0023): what a batch of output tiles needs
//! to apply its neighbourhood filters, and the GPU passes doing it.
//!
//! For the filters f1..fn of a program, in order, the output tiles need the
//! filtered tiles of fn; filtering fn on a tile reads its stage (the program
//! cut before fn) on the tiles around, within the margin of fn; that stage
//! reads the filtered tiles of f1..fn-1 there, and so on. The regions grow
//! from the last filter to the first:
//!
//! - `need(n)` is the set of output tiles;
//! - `stage(j)` is `need(j)` dilated by the margin of fj, within the level;
//! - `need(j - 1)` is `stage(j)`.
//!
//! The passes then run from f1 to fn: the stage tiles of fj are composed by
//! the composite pass into the stage pool, blurred horizontally into the
//! scratch pool (on the tiles of `need(j)` dilated vertically), then
//! vertically into the filtered pool, where the unsharp mask also reads the
//! stage. Every intermediate result is stored in the compute format (0026).

use std::collections::{BTreeSet, HashMap};

use cygnus_photo_core::pixels::{TILE_SIZE, TileCoord};
use cygnus_photo_core::{Document, Filter, NodeId, NodeKind};

use crate::program::{self, At, Op};
use crate::renderer::{Pool, RenderError};
use crate::sources::SourceKey;

/// Tiles of one filter pool at most: a batch whose regions need more is
/// split.
pub(crate) const MAX_FILTER_TILES: usize = 256;

/// Filtered tiles of one batch at most: they are copied to slots reserved
/// at the end of the layer pools, which the composite pass reads (0040).
pub(crate) const MAX_FILTERED_TILES: usize = 64;

/// One filter of a batch and the tiles it works on.
pub(crate) struct FilterStage {
    pub(crate) id: NodeId,
    pub(crate) filter: Filter,
    /// Pixels read on each side, at the level of the batch.
    pub(crate) margin: u32,
    /// Tiles whose filtered value is needed.
    pub(crate) need: Vec<TileCoord>,
    /// Tiles of the blur input after the horizontal pass: `need` dilated
    /// vertically by the margin.
    pub(crate) rows: Vec<TileCoord>,
    /// Stage tiles: their coordinates and the program cut before the
    /// filter (tiles whose program lacks the filter have none and read as
    /// transparent).
    pub(crate) region: Vec<(TileCoord, Vec<Op>)>,
}

/// The filter stages of a batch, and the layer and mask tiles every one of
/// its programs reads.
pub(crate) struct StagePlan {
    pub(crate) stages: Vec<FilterStage>,
    pub(crate) layers: BTreeSet<SourceKey>,
    pub(crate) masks: BTreeSet<SourceKey>,
}

impl StagePlan {
    /// Tiles the stage, scratch and filtered pools need at once.
    pub(crate) fn pool_tiles(&self) -> (usize, usize, usize) {
        let stage = self
            .stages
            .iter()
            .map(|s| s.region.len())
            .max()
            .unwrap_or(0);
        let rows = self.stages.iter().map(|s| s.rows.len()).max().unwrap_or(0);
        let filtered = self.stages.iter().map(|s| s.need.len()).sum();
        (stage, rows, filtered)
    }

    pub(crate) fn fits(&self) -> bool {
        let (stage, rows, filtered) = self.pool_tiles();
        stage <= MAX_FILTER_TILES && rows <= MAX_FILTER_TILES && filtered <= MAX_FILTERED_TILES
    }
}

/// Tiles per row and column of pyramid `level` of `document`.
fn level_tiles(document: &Document, level: u32) -> (u32, u32) {
    let factor = 1u32 << level;
    (
        document.width.div_ceil(factor).div_ceil(TILE_SIZE),
        document.height.div_ceil(factor).div_ceil(TILE_SIZE),
    )
}

/// `tiles` dilated by `dx` and `dy` tiles, within the level.
fn dilate(tiles: &BTreeSet<TileCoord>, dx: u32, dy: u32, limit: (u32, u32)) -> BTreeSet<TileCoord> {
    let mut out = BTreeSet::new();
    for coord in tiles {
        let x0 = coord.tx.saturating_sub(dx);
        let y0 = coord.ty.saturating_sub(dy);
        let x1 = (coord.tx + dx).min(limit.0.saturating_sub(1));
        let y1 = (coord.ty + dy).min(limit.1.saturating_sub(1));
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                out.insert(TileCoord { tx, ty });
            }
        }
    }
    out
}

/// The filter stages needed by `outputs` at `level`, whose programs are in
/// `programs` (computed there for the other tiles). `None` when no program
/// has a filter.
pub(crate) fn plan(
    document: &Document,
    outputs: &[TileCoord],
    level: u32,
    programs: &mut HashMap<TileCoord, Vec<Op>>,
) -> Result<Option<StagePlan>, RenderError> {
    let mut program_at = |coord: TileCoord| -> Result<Vec<Op>, RenderError> {
        if let Some(ops) = programs.get(&coord) {
            return Ok(ops.clone());
        }
        let ops = program::tile_program_at(document, At { coord, level })?;
        programs.insert(coord, ops.clone());
        Ok(ops)
    };
    // The filters of the batch, in program order.
    let mut ids: Vec<NodeId> = Vec::new();
    for coord in outputs {
        for id in program::filters(&program_at(*coord)?) {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    }
    if ids.is_empty() {
        return Ok(None);
    }
    let limit = level_tiles(document, level);
    let mut need: BTreeSet<TileCoord> = outputs.iter().copied().collect();
    let mut stages = Vec::with_capacity(ids.len());
    for id in ids.iter().rev() {
        let Some(NodeKind::Filter { filter }) = document.node(*id).map(|node| &node.kind) else {
            return Err(RenderError::Unsupported(
                "a filter operation without its layer",
            ));
        };
        let margin = program::filter_margin(filter, level);
        let reach = margin.div_ceil(TILE_SIZE);
        let region_coords = dilate(&need, reach, reach, limit);
        let rows = dilate(&need, 0, reach, limit);
        let mut region = Vec::with_capacity(region_coords.len());
        for coord in &region_coords {
            if let Some(stage) = program::stage(&program_at(*coord)?, *id) {
                region.push((*coord, stage.to_vec()));
            }
        }
        stages.push(FilterStage {
            id: *id,
            filter: *filter,
            margin,
            need: need.iter().copied().collect(),
            rows: rows.into_iter().collect(),
            region,
        });
        need = region_coords;
    }
    stages.reverse();
    // Every program read: the outputs and the stages, which are prefixes
    // of the programs of their tiles.
    let mut layers = BTreeSet::new();
    let mut masks = BTreeSet::new();
    let mut coords: BTreeSet<TileCoord> = outputs.iter().copied().collect();
    if let Some(first) = stages.first() {
        coords.extend(first.region.iter().map(|(coord, _)| *coord));
    }
    for coord in coords {
        let (l, m) = program::sources(&program_at(coord)?);
        layers.extend(l);
        masks.extend(m);
    }
    Ok(Some(StagePlan {
        stages,
        layers,
        masks,
    }))
}

/// One-sided weights of the Gaussian of standard deviation `sigma` over
/// `radius` taps on each side, normalized so that the whole kernel sums to
/// one.
pub(crate) fn kernel(sigma: f64, radius: u32) -> Vec<f32> {
    let sigma = sigma.max(1e-3);
    let raw: Vec<f64> = (0..=radius)
        .map(|i| (-(f64::from(i) * f64::from(i)) / (2.0 * sigma * sigma)).exp())
        .collect();
    let total = raw[0] + 2.0 * raw[1..].iter().sum::<f64>();
    raw.iter().map(|w| (w / total) as f32).collect()
}

/// Standard deviation of `filter` at `level`, in its pixels.
pub(crate) fn sigma(filter: &Filter, level: u32) -> f64 {
    let radius = match *filter {
        Filter::GaussianBlur { radius } | Filter::UnsharpMask { radius, .. } => radius,
    };
    f64::from(radius) / f64::from(1u32 << level)
}

/// Words of the parameters of the filter passes (`Params` in WGSL).
pub(crate) fn params(document: &Document, stage: &FilterStage, level: u32) -> Vec<u32> {
    let factor = 1u32 << level;
    let (kind, amount, threshold) = match stage.filter {
        Filter::GaussianBlur { .. } => (0u32, 0.0f32, 0.0f32),
        Filter::UnsharpMask {
            amount, threshold, ..
        } => (1, amount, threshold / 255.0),
    };
    vec![
        document.width.div_ceil(factor),
        document.height.div_ceil(factor),
        stage.margin,
        kind,
        amount.to_bits(),
        threshold.to_bits(),
        0,
        0,
    ]
}

/// Page table of `slots` (coordinates and slots) as the filter shader reads
/// it: the bounding box, then one slot or -1 per tile.
pub(crate) fn page_table(slots: &[(TileCoord, u32)]) -> Vec<i32> {
    let Some(x0) = slots.iter().map(|(c, _)| c.tx).min() else {
        return vec![0, 0, 0, 0];
    };
    let y0 = slots.iter().map(|(c, _)| c.ty).min().unwrap_or(0);
    let x1 = slots.iter().map(|(c, _)| c.tx).max().unwrap_or(0);
    let y1 = slots.iter().map(|(c, _)| c.ty).max().unwrap_or(0);
    let (w, h) = (x1 - x0 + 1, y1 - y0 + 1);
    let mut table = vec![x0 as i32, y0 as i32, w as i32, h as i32];
    table.resize(4 + (w * h) as usize, -1);
    for (coord, slot) in slots {
        table[4 + ((coord.ty - y0) * w + (coord.tx - x0)) as usize] = *slot as i32;
    }
    table
}

/// The pools and pipelines of the filter passes. The pools grow on demand
/// up to [`MAX_FILTER_TILES`] tiles; the filtered pool is always bound by
/// the composite pass, so it starts with one tile.
pub(crate) struct FilterPasses {
    /// Compiled on the first filter met: a document without filters never
    /// builds them.
    blur: Option<Blur>,
    pub(crate) stage: Grown,
    pub(crate) rows: Grown,
    pub(crate) filtered: Grown,
}

/// The two passes of the blur.
pub(crate) struct Blur {
    pub(crate) h: wgpu::ComputePipeline,
    pub(crate) v: wgpu::ComputePipeline,
}

/// A pool and its number of slots.
pub(crate) struct Grown {
    /// Allocated on the first filter met, with 16 tiles at least.
    pool: Option<Pool>,
    slots: usize,
    label: &'static str,
}

impl Grown {
    fn new(label: &'static str) -> Self {
        Self {
            pool: None,
            slots: 0,
            label,
        }
    }

    /// Makes room for `wanted` tiles, dropping the content.
    pub(crate) fn reserve(&mut self, device: &wgpu::Device, wanted: usize) {
        if self.pool.is_none() || wanted > self.slots {
            let slots = wanted.next_power_of_two().clamp(16, MAX_FILTER_TILES);
            self.pool = Some(Pool::with_usage(
                device,
                self.label,
                slots as u32,
                wgpu::TextureUsages::COPY_SRC,
            ));
            self.slots = slots;
        }
    }

    /// The pool, once [`Self::reserve`] has run.
    pub(crate) fn pool(&self) -> Result<&Pool, RenderError> {
        self.pool.as_ref().ok_or(RenderError::Unsupported(
            "a filter pool used before it is made",
        ))
    }
}

impl FilterPasses {
    pub(crate) fn new() -> Self {
        Self {
            blur: None,
            stage: Grown::new("cygnus filter stage pool"),
            rows: Grown::new("cygnus filter rows pool"),
            filtered: Grown::new("cygnus filtered pool"),
        }
    }

    /// Compiles the blur passes if needed.
    pub(crate) fn prepare(&mut self, device: &wgpu::Device) {
        if self.blur.is_some() {
            return;
        }
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cygnus filter"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::filter().into()),
        });
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        self.blur = Some(Blur {
            h: pipeline("blur_h"),
            v: pipeline("blur_v"),
        });
    }

    /// The blur passes, once [`Self::prepare`] has run.
    pub(crate) fn blur(&self) -> Option<&Blur> {
        self.blur.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_kernel_sums_to_one_and_follows_the_gaussian() {
        let weights = kernel(2.0, 6);
        let total = weights[0] + 2.0 * weights[1..].iter().sum::<f32>();
        assert!((total - 1.0).abs() < 1e-6);
        // Ratio of neighbours: exp(-(2i + 1) / (2 σ²)).
        let ratio = f64::from(weights[2] / weights[1]);
        assert!((ratio - (-3.0f64 / 8.0).exp()).abs() < 1e-5);
    }

    #[test]
    fn a_tiny_radius_is_the_identity() {
        let weights = kernel(0.1, 1);
        assert!(weights[0] > 0.999_999);
    }

    #[test]
    fn page_tables_cover_the_bounding_box() {
        let table = page_table(&[
            (TileCoord { tx: 3, ty: 5 }, 7),
            (TileCoord { tx: 4, ty: 6 }, 2),
        ]);
        assert_eq!(&table[..4], &[3, 5, 2, 2]);
        assert_eq!(&table[4..], &[7, -1, -1, 2]);
    }

    #[test]
    fn dilation_stays_within_the_level() {
        let tiles: BTreeSet<TileCoord> = [TileCoord { tx: 0, ty: 1 }].into();
        let grown = dilate(&tiles, 1, 2, (3, 3));
        assert_eq!(grown.len(), 2 * 3);
        assert!(grown.iter().all(|c| c.tx <= 1 && c.ty <= 2));
    }
}

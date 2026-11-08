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

//! Tile renderer: composites layer tiles and exports the result
//! (docs/decisions/0011, 0012 and 0021).
//!
//! The document is processed in batches of output tiles, so that it never
//! has to be resident on the GPU at once. Each batch makes the layer and
//! mask tiles it needs resident in the source pools (integers, as stored in
//! the document; [`crate::sources`]), runs the composition program of each
//! output tile ([`crate::program`]) into a pool of the compute format, then
//! converts the composites to integers for the readback. Every GPU error is
//! caught by an error scope and returned, never turned into a crash.

use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use cygnus_photo_core::pixels::{TILE_SIZE, Tile, TileCoord, tiles_in};
use cygnus_photo_core::{Document, NodeId, NodeKind};

use crate::adjust::{self, CurveTables};
use crate::filter::{self, FilterPasses};
use crate::format::COMPUTE_FORMAT;
use crate::image::{ExportBits, Image};
use crate::preview::ReducedCache;
use crate::program::{self, Op};
use crate::shaders;
use crate::sources::{PoolKind, SourceKey, SourceStats, TilePool};

/// Source pool budget of [`Renderer::new`]: that of an integrated GPU.
pub const DEFAULT_SOURCE_BUDGET: u64 = 256 * 1024 * 1024;

/// Bytes of one layer tile and one mask tile in their pools.
const LAYER_TILE_BYTES: u64 = (TILE_SIZE * TILE_SIZE * crate::format::SOURCE_TEXEL_BYTES) as u64;
const MASK_TILE_BYTES: u64 = (TILE_SIZE * TILE_SIZE * crate::format::MASK_TEXEL_BYTES) as u64;

/// Slots of the layer and mask pools for `budget` bytes: four fifths for
/// layer tiles, one fifth for mask tiles, each pool within `max_layers`
/// array layers and holding at least 16 tiles.
pub fn source_slots(budget: u64, max_layers: u32) -> (u32, u32) {
    let slots = |bytes: u64, tile: u64| -> u32 {
        u32::try_from(bytes / tile)
            .unwrap_or(u32::MAX)
            .clamp(16, max_layers.max(16))
            .min(max_layers)
    };
    (
        slots(budget / 5 * 4, LAYER_TILE_BYTES),
        slots(budget / 5, MASK_TILE_BYTES),
    )
}

/// Words of one operation in the composite shader (`Op`).
const OP_WORDS: usize = 8;
/// Slot value meaning "no mask tile" (`NO_SLOT` in WGSL).
const NO_SLOT: u32 = u32::MAX;
/// Slots of the composite pool: output tiles of one batch.
const COMPOSITE_SLOTS: u32 = 64;
/// Workgroup side, as in the shaders.
const WORKGROUP: u32 = 16;
/// Pixels per tile.
const TILE_PIXELS: u32 = TILE_SIZE * TILE_SIZE;

/// Why a document could not be rendered.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// A structure the renderer does not support yet.
    #[error("{0} is not supported yet")]
    Unsupported(&'static str),
    /// A tile reads more layer or mask tiles than a source pool holds.
    #[error("{count} layer or mask tiles overlap one tile, more than the source pool holds")]
    TooManyLayers {
        /// Number of tiles read.
        count: usize,
    },
    /// The GPU reported an error.
    #[error("GPU error: {0}")]
    Gpu(String),
}

pub(crate) struct Pool {
    pub(crate) texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
}

impl Pool {
    pub(crate) fn new(device: &wgpu::Device, label: &str, slots: u32) -> Self {
        Self::with_usage(device, label, slots, wgpu::TextureUsages::empty())
    }

    /// A pool whose texture also allows `extra` usages (copies).
    pub(crate) fn with_usage(
        device: &wgpu::Device,
        label: &str,
        slots: u32,
        extra: wgpu::TextureUsages,
    ) -> Self {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: slots,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COMPUTE_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | extra,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        Self { texture, view }
    }
}

pub(crate) fn pipeline(
    device: &wgpu::Device,
    label: &str,
    source: String,
) -> wgpu::ComputePipeline {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some(label),
        layout: None,
        module: &module,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    })
}

/// Source pools and their use clock.
struct Sources {
    layers: TilePool,
    masks: TilePool,
    clock: u64,
    stats: SourceStats,
    curves: CurveTables,
    /// Reduced tiles of the approximate tiles (docs/decisions/0034, T3):
    /// computed on the processor, uploaded to their own pools.
    reduced: ReducedCache,
    reduced_layers: TilePool,
    reduced_masks: TilePool,
    /// Pools and pipelines of the filter stages.
    filters: FilterPasses,
}

/// Tile renderer bound to one GPU device.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    composite: wgpu::ComputePipeline,
    export: wgpu::ComputePipeline,
    sources: Mutex<Sources>,
    composites: Pool,
}

/// One output tile of a batch: where it goes, and its program.
pub(crate) struct OutputTile {
    pub(crate) coord: TileCoord,
    pub(crate) target: u32,
    ops: Vec<Op>,
}

/// Output tiles whose layer and mask tiles fit in the source pools
/// together.
pub(crate) struct Batch {
    /// Pyramid level of the outputs: 0, or a preview level read from
    /// reduced sources.
    pub(crate) level: u32,
    pub(crate) outputs: Vec<OutputTile>,
    layers: BTreeSet<SourceKey>,
    masks: BTreeSet<SourceKey>,
    /// Filter stages to run before the outputs (docs/decisions/0023).
    stages: Vec<filter::FilterStage>,
}

impl Batch {
    fn new(level: u32) -> Self {
        Self {
            level,
            outputs: Vec::new(),
            layers: BTreeSet::new(),
            masks: BTreeSet::new(),
            stages: Vec::new(),
        }
    }
}

impl Renderer {
    /// Creates the pipelines and pools on `device`, with source pools sized
    /// from [`DEFAULT_SOURCE_BUDGET`] (see [`source_slots`]).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, RenderError> {
        Self::with_memory_budget(device, queue, DEFAULT_SOURCE_BUDGET)
    }

    /// Creates the renderer with source pools sized from `budget` bytes of
    /// GPU memory (see [`crate::gpu::source_memory_budget`]).
    pub fn with_memory_budget(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        budget: u64,
    ) -> Result<Self, RenderError> {
        let (layers, masks) = source_slots(budget, device.limits().max_texture_array_layers);
        Self::with_source_slots(device, queue, layers, masks)
    }

    /// Creates the renderer with source pools of `layer_slots` layer tiles
    /// and `mask_slots` mask tiles (at least one each), for tests of
    /// residency under a reduced budget.
    pub fn with_source_slots(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        layer_slots: u32,
        mask_slots: u32,
    ) -> Result<Self, RenderError> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let layers = TilePool::new(device, PoolKind::Layers, layer_slots.max(1));
        let masks = TilePool::new(device, PoolKind::Masks, mask_slots.max(1));
        let stats = SourceStats {
            capacity: (layers.capacity(), masks.capacity()),
            ..SourceStats::default()
        };
        let renderer = Self {
            device: device.clone(),
            queue: queue.clone(),
            composite: pipeline(device, "cygnus composite", shaders::composite()),
            export: pipeline(device, "cygnus export", shaders::export()),
            sources: Mutex::new(Sources {
                layers,
                masks,
                clock: 0,
                stats,
                curves: CurveTables::new(device, queue),
                reduced: ReducedCache::new(),
                reduced_layers: TilePool::new(device, PoolKind::Layers, (layer_slots / 4).max(16)),
                reduced_masks: TilePool::new(device, PoolKind::Masks, (mask_slots / 4).max(16)),
                filters: FilterPasses::new(device),
            }),
            composites: Pool::new(device, "cygnus composite pool", COMPOSITE_SLOTS),
        };
        if let Some(error) = pollster::block_on(memory.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        match pollster::block_on(scope.pop()) {
            Some(error) => Err(RenderError::Gpu(error.to_string())),
            None => Ok(renderer),
        }
    }

    /// Counters of the source pools and of the curve lookup tables.
    pub fn source_stats(&self) -> SourceStats {
        let sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        SourceStats {
            curve_tables: sources.curves.stats,
            reduced: sources.reduced.stats.built,
            reduced_bytes: sources.reduced.stats.bytes,
            ..sources.stats
        }
    }

    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Splits the composition of `outputs` into batches whose layer and
    /// mask tiles fit in the source pools together.
    /// `target(index_in_batch, coord)` gives the destination slot of each
    /// output tile; `max_outputs` bounds a batch.
    pub(crate) fn plan(
        &self,
        document: &Document,
        outputs: impl IntoIterator<Item = TileCoord>,
        max_outputs: usize,
        target: impl FnMut(usize, TileCoord) -> u32,
    ) -> Result<Vec<Batch>, RenderError> {
        self.plan_at(document, outputs, max_outputs, target, 0)
    }

    /// Like [`Self::plan`] for output tiles of pyramid `level`, composed
    /// from reduced sources when `level` is above 0.
    pub(crate) fn plan_at(
        &self,
        document: &Document,
        outputs: impl IntoIterator<Item = TileCoord>,
        max_outputs: usize,
        mut target: impl FnMut(usize, TileCoord) -> u32,
        level: u32,
    ) -> Result<Vec<Batch>, RenderError> {
        let (layer_slots, mask_slots) = {
            let sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
            if level == 0 {
                (
                    sources.layers.capacity() as usize,
                    sources.masks.capacity() as usize,
                )
            } else {
                (
                    sources.reduced_layers.capacity() as usize,
                    sources.reduced_masks.capacity() as usize,
                )
            }
        };
        let outputs: Vec<TileCoord> = outputs.into_iter().collect();
        let mut programs: HashMap<TileCoord, Vec<Op>> = HashMap::new();
        for coord in &outputs {
            programs.insert(
                *coord,
                program::tile_program_at(
                    document,
                    program::At {
                        coord: *coord,
                        level,
                    },
                )?,
            );
        }
        let filtered = outputs.iter().any(|coord| {
            programs
                .get(coord)
                .is_some_and(|ops| !program::filters(ops).is_empty())
        });
        if filtered {
            return self.plan_filtered(
                document,
                &outputs,
                max_outputs,
                target,
                level,
                (layer_slots, mask_slots),
                &mut programs,
            );
        }
        let mut batches = Vec::new();
        let mut batch = Batch::new(level);
        for coord in outputs {
            let ops = programs.remove(&coord).unwrap_or_default();
            let (layers, masks) = program::sources(&ops);
            for (count, slots) in [(layers.len(), layer_slots), (masks.len(), mask_slots)] {
                if count > slots {
                    return Err(RenderError::TooManyLayers { count });
                }
            }
            let new_layers = layers.iter().filter(|k| !batch.layers.contains(k)).count();
            let new_masks = masks.iter().filter(|k| !batch.masks.contains(k)).count();
            let full = batch.outputs.len() == max_outputs
                || batch.layers.len() + new_layers > layer_slots
                || batch.masks.len() + new_masks > mask_slots;
            if full {
                batches.push(std::mem::replace(&mut batch, Batch::new(level)));
            }
            batch.layers.extend(layers);
            batch.masks.extend(masks);
            let index = batch.outputs.len();
            batch.outputs.push(OutputTile {
                coord,
                target: target(index, coord),
                ops,
            });
        }
        if !batch.outputs.is_empty() {
            batches.push(batch);
        }
        Ok(batches)
    }

    /// [`Self::plan_at`] for programs with filters: each batch also holds
    /// its filter stages, whose tiles around the outputs must fit in the
    /// filter pools, and whose sources must fit in the source pools with
    /// those of the outputs.
    #[allow(clippy::too_many_arguments)]
    fn plan_filtered(
        &self,
        document: &Document,
        outputs: &[TileCoord],
        max_outputs: usize,
        mut target: impl FnMut(usize, TileCoord) -> u32,
        level: u32,
        (layer_slots, mask_slots): (usize, usize),
        programs: &mut HashMap<TileCoord, Vec<Op>>,
    ) -> Result<Vec<Batch>, RenderError> {
        let fits = |plan: &filter::StagePlan| {
            plan.fits() && plan.layers.len() <= layer_slots && plan.masks.len() <= mask_slots
        };
        let too_far = |plan: &filter::StagePlan| {
            if plan.fits() {
                RenderError::TooManyLayers {
                    count: plan.layers.len().max(plan.masks.len()),
                }
            } else {
                RenderError::Unsupported("filters reaching further than their GPU pools hold")
            }
        };
        let mut groups: Vec<(Vec<TileCoord>, filter::StagePlan)> = Vec::new();
        let mut current: Vec<TileCoord> = Vec::new();
        let mut current_plan: Option<filter::StagePlan> = None;
        for coord in outputs {
            let mut candidate = current.clone();
            candidate.push(*coord);
            let plan = filter::plan(document, &candidate, level, programs)?;
            let accepted = candidate.len() <= max_outputs && plan.as_ref().is_none_or(fits);
            if accepted {
                current = candidate;
                current_plan = plan;
                continue;
            }
            if let Some(plan) = plan.as_ref()
                && current.is_empty()
            {
                return Err(too_far(plan));
            }
            if let Some(plan) = current_plan.take() {
                groups.push((std::mem::take(&mut current), plan));
            }
            let alone = filter::plan(document, &[*coord], level, programs)?;
            if let Some(plan) = alone.as_ref()
                && !fits(plan)
            {
                return Err(too_far(plan));
            }
            current = vec![*coord];
            current_plan = alone;
        }
        if let Some(plan) = current_plan.take() {
            groups.push((current, plan));
        }
        let mut batches = Vec::with_capacity(groups.len());
        for (coords, plan) in groups {
            let mut batch = Batch::new(level);
            for (index, coord) in coords.into_iter().enumerate() {
                batch.outputs.push(OutputTile {
                    coord,
                    target: target(index, coord),
                    ops: programs.get(&coord).cloned().unwrap_or_default(),
                });
            }
            batch.layers = plan.layers;
            batch.masks = plan.masks;
            batch.stages = plan.stages;
            batches.push(batch);
        }
        Ok(batches)
    }

    /// Starts a frame of the canvas: takes in the reduced tiles computed
    /// since the previous one.
    pub(crate) fn begin_frame(&self, document: &Document) {
        let mut sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        sources.reduced.begin_frame(document);
    }

    /// Cancels the reductions requested and not computed yet.
    pub(crate) fn cancel_reductions(&self) {
        let mut sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        sources.reduced.cancel();
    }

    /// Memory budget of the reduced sources, in bytes.
    pub(crate) fn reduced_budget(&self) -> u64 {
        let sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        sources.reduced.budget()
    }

    /// Whether the reduced sources of the output tile `coord` of `level`
    /// are computed; requests the missing ones. With `wait`, waits for
    /// them. A tile whose program fails counts as ready: the composition
    /// reports the error.
    pub(crate) fn reduced_ready(
        &self,
        document: &Document,
        coord: TileCoord,
        level: u32,
        wait: bool,
    ) -> bool {
        let Ok(ops) = program::tile_program_at(document, program::At { coord, level }) else {
            return true;
        };
        let (mut layers, mut masks) = program::sources(&ops);
        if !program::filters(&ops).is_empty() {
            // Filter stages read the sources of the tiles around too.
            let mut programs = HashMap::from([(coord, ops)]);
            match filter::plan(document, &[coord], level, &mut programs) {
                Ok(Some(plan)) => {
                    layers = plan.layers.into_iter().collect();
                    masks = plan.masks.into_iter().collect();
                }
                Ok(None) => {}
                Err(_) => return true,
            }
        }
        let mut sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        let layers_ready = sources
            .reduced
            .ready_or_wait(document, &layers, level, wait);
        let masks_ready = sources.reduced.ready_or_wait(document, &masks, level, wait);
        layers_ready && masks_ready
    }

    /// Makes the sources of the output tiles `coords` resident while the
    /// canvas is idle (docs/decisions/0027, point 1): resident ones are
    /// marked used, at most `max_uploads` missing ones are uploaded. Does
    /// nothing if they do not fit in the pools. Returns the uploads.
    pub(crate) fn prefetch(
        &self,
        document: &Document,
        coords: &[TileCoord],
        max_uploads: usize,
    ) -> Result<usize, RenderError> {
        let mut layers = BTreeSet::new();
        let mut masks = BTreeSet::new();
        for coord in coords {
            let ops = program::tile_program_at(
                document,
                program::At {
                    coord: *coord,
                    level: 0,
                },
            )?;
            let (l, m) = program::sources(&ops);
            layers.extend(l);
            masks.extend(m);
        }
        let mut sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        let sources = &mut *sources;
        if layers.len() > sources.layers.capacity() as usize
            || masks.len() > sources.masks.capacity() as usize
        {
            return Ok(0);
        }
        sources.clock += 1;
        let clock = sources.clock;
        let mut uploads = 0;
        for (keys, pool) in [(&layers, &mut sources.layers), (&masks, &mut sources.masks)] {
            let mut wanted = Vec::new();
            for key in keys {
                if let Some(tile) = program::layer_tile(document, key.0, key.1) {
                    let resident = pool.is_resident(key, tile);
                    if resident || uploads < max_uploads {
                        uploads += usize::from(!resident);
                        wanted.push((*key, tile));
                    }
                }
            }
            pool.ensure(&self.queue, &wanted, clock, &mut sources.stats)?;
        }
        Ok(uploads)
    }

    /// Renders the whole document and returns it as an image: the export
    /// path, also used by the tests and the command line.
    pub fn export(&self, document: &Document, bits: ExportBits) -> Result<Image, RenderError> {
        let mut image = Image {
            width: document.width,
            height: document.height,
            bits,
            data: vec![0; document.width as usize * document.height as usize * 4],
        };
        let batches = self.plan(
            document,
            tiles_in(document.bounds()),
            COMPOSITE_SLOTS as usize,
            |index, _| index as u32,
        )?;
        for batch in &batches {
            self.export_batch(document, batch, &mut image)?;
        }
        Ok(image)
    }

    fn storage_bytes(&self, label: &str, bytes: &[u8]) -> wgpu::Buffer {
        // The buffer is written through the queue, not mapped at creation:
        // wgpu's mapped initialization panics when the GPU is out of memory,
        // while a queue write reports the error to the error scopes like any
        // other.
        let bytes = padded(bytes);
        let bytes = bytes.as_ref();
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: bytes.len() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.queue.write_buffer(&buffer, 0, bytes);
        buffer
    }

    pub(crate) fn storage(&self, label: &str, words: &[u32]) -> wgpu::Buffer {
        self.storage_bytes(label, bytemuck::cast_slice(words))
    }

    pub(crate) fn bind(
        &self,
        pipeline: &wgpu::ComputePipeline,
        resources: &[wgpu::BindingResource<'_>],
    ) -> wgpu::BindGroup {
        let entries: Vec<wgpu::BindGroupEntry<'_>> = resources
            .iter()
            .enumerate()
            .map(|(binding, resource)| wgpu::BindGroupEntry {
                binding: binding as u32,
                resource: resource.clone(),
            })
            .collect();
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    }

    fn dispatch(
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::ComputePipeline,
        group: &wgpu::BindGroup,
        tiles: u32,
    ) {
        let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, group, &[]);
        pass.dispatch_workgroups(TILE_SIZE / WORKGROUP, TILE_SIZE / WORKGROUP, tiles);
    }

    /// Makes the source tiles of `batch` resident, then records its
    /// composite pass into `encoder`, writing each output tile to its target
    /// slot of `target`.
    ///
    /// Uploads take effect before the next submission: the caller must
    /// submit the work recorded by a previous call before calling again, so
    /// that a slot reused here is not read by older work.
    pub(crate) fn record_composite(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        document: &Document,
        batch: &Batch,
        target: &wgpu::TextureView,
    ) -> Result<(), RenderError> {
        let mut sources = self.sources.lock().unwrap_or_else(PoisonError::into_inner);
        let sources = &mut *sources;
        sources.clock += 1;
        let tiles =
            |keys: &BTreeSet<SourceKey>| -> Result<Vec<(SourceKey, &Arc<Tile>)>, RenderError> {
                keys.iter()
                    .map(|key| {
                        program::layer_tile(document, key.0, key.1)
                            .map(|tile| (*key, tile))
                            .ok_or(RenderError::Unsupported(
                                "a source tile missing from its document",
                            ))
                    })
                    .collect()
            };
        let (layer_slots, mask_slots) = if batch.level == 0 {
            (
                sources.layers.ensure(
                    &self.queue,
                    &tiles(&batch.layers)?,
                    sources.clock,
                    &mut sources.stats,
                )?,
                sources.masks.ensure(
                    &self.queue,
                    &tiles(&batch.masks)?,
                    sources.clock,
                    &mut sources.stats,
                )?,
            )
        } else {
            // Reduced tiles, current since the canvas checked them.
            let reduced = |cache: &mut ReducedCache,
                           keys: &BTreeSet<SourceKey>|
             -> Result<Vec<(SourceKey, Arc<Tile>)>, RenderError> {
                keys.iter()
                    .map(|key| {
                        cache
                            .get(document, *key, batch.level)
                            .map(|tile| (*key, tile))
                            .ok_or(RenderError::Unsupported(
                                "composing before its reduced sources are computed",
                            ))
                    })
                    .collect()
            };
            let layers = reduced(&mut sources.reduced, &batch.layers)?;
            let masks = reduced(&mut sources.reduced, &batch.masks)?;
            let layer_refs: Vec<(SourceKey, &Arc<Tile>)> =
                layers.iter().map(|(key, tile)| (*key, tile)).collect();
            let mask_refs: Vec<(SourceKey, &Arc<Tile>)> =
                masks.iter().map(|(key, tile)| (*key, tile)).collect();
            let mut uploads = SourceStats::default();
            let slots = (
                sources.reduced_layers.ensure(
                    &self.queue,
                    &layer_refs,
                    sources.clock,
                    &mut uploads,
                )?,
                sources.reduced_masks.ensure(
                    &self.queue,
                    &mask_refs,
                    sources.clock,
                    &mut uploads,
                )?,
            );
            sources.stats.reduced_uploads += uploads.uploads;
            slots
        };
        let (layers_view, masks_view) = if batch.level == 0 {
            (&sources.layers.view, &sources.masks.view)
        } else {
            (&sources.reduced_layers.view, &sources.reduced_masks.view)
        };

        // Adjustments of the batch, stages included: their curve tables
        // made resident, then their parameters encoded once each, at a word
        // offset.
        let programs = batch.outputs.iter().map(|tile| tile.ops.as_slice()).chain(
            batch
                .stages
                .iter()
                .flat_map(|stage| stage.region.iter().map(|(_, ops)| ops.as_slice())),
        );
        let mut adjustments: Vec<(NodeId, &cygnus_photo_core::Adjustment)> = Vec::new();
        for op in programs.flatten() {
            if op.code == program::Code::Adjust
                && let Some(id) = op.adjustment
                && !adjustments.iter().any(|(seen, _)| *seen == id)
                && let Some(NodeKind::Adjustment { adjustment }) =
                    document.node(id).map(|node| &node.kind)
            {
                adjustments.push((id, adjustment));
            }
        }
        let curves: Vec<_> = adjustments
            .iter()
            .flat_map(|(_, adjustment)| adjust::curves(adjustment))
            .collect();
        sources.curves.prepare(&curves);
        let mut params: Vec<u32> = Vec::new();
        let mut placed: HashMap<NodeId, (u32, u32)> = HashMap::new();
        for (id, adjustment) in &adjustments {
            let offset = params.len() as u32;
            adjust::encode(adjustment, &sources.curves, &mut params)?;
            placed.insert(*id, (offset, adjust::kind(adjustment)));
        }
        if !batch.stages.is_empty() {
            let (stage_tiles, rows, filtered) = (
                batch
                    .stages
                    .iter()
                    .map(|s| s.region.len())
                    .max()
                    .unwrap_or(0),
                batch.stages.iter().map(|s| s.rows.len()).max().unwrap_or(0),
                batch.stages.iter().map(|s| s.need.len()).sum(),
            );
            sources.filters.stage.reserve(&self.device, stage_tiles);
            sources.filters.rows.reserve(&self.device, rows);
            sources.filters.filtered.reserve(&self.device, filtered);
        }
        let params_buffer = self.storage("cygnus adjustment parameters", &params);

        // One job per tile, eight words per operation. A filter operation
        // reads the filtered tile of its filter at the same place.
        let encode = |tiles: &[(u32, &[Op], TileCoord)],
                      filtered: &HashMap<(NodeId, TileCoord), u32>|
         -> Result<(Vec<u32>, Vec<u32>), RenderError> {
            let mut ops: Vec<u32> = Vec::new();
            let mut jobs: Vec<u32> = Vec::new();
            for (target, program, coord) in tiles {
                jobs.extend([
                    (ops.len() / OP_WORDS) as u32,
                    program.len() as u32,
                    *target,
                    0,
                ]);
                for op in *program {
                    // An adjustment reads its parameters at `slot`, its kind
                    // in the flags.
                    let (slot, kind) = match (op.code, op.adjustment) {
                        (program::Code::Filter, Some(id)) => (
                            filtered.get(&(id, *coord)).copied().ok_or(
                                RenderError::Unsupported("a filter operation without its result"),
                            )?,
                            0,
                        ),
                        (_, Some(id)) => placed.get(&id).copied().ok_or(
                            RenderError::Unsupported("an adjustment operation without its layer"),
                        )?,
                        (_, None) => (
                            op.source
                                .and_then(|key| layer_slots.get(&key).copied())
                                .unwrap_or(0),
                            0,
                        ),
                    };
                    let mask_slot = op
                        .mask
                        .tile
                        .and_then(|key| mask_slots.get(&key).copied())
                        .unwrap_or(NO_SLOT);
                    ops.extend([
                        op.code as u32,
                        slot,
                        op.mode.code(),
                        op.opacity.to_bits(),
                        mask_slot,
                        op.mask.value.to_bits(),
                        op.mask.density.to_bits(),
                        u32::from(op.mask.inverted) | kind << 8,
                    ]);
                }
            }
            Ok((ops, jobs))
        };
        let filtered_view = &sources.filters.filtered.pool.view;
        let composite = |encoder: &mut wgpu::CommandEncoder,
                         tiles: &[(u32, &[Op], TileCoord)],
                         filtered: &HashMap<(NodeId, TileCoord), u32>,
                         target: &wgpu::TextureView|
         -> Result<(), RenderError> {
            let (ops, jobs) = encode(tiles, filtered)?;
            let ops_buffer = self.storage("cygnus composite ops", &ops);
            let jobs_buffer = self.storage("cygnus composite jobs", &jobs);
            let group = self.bind(
                &self.composite,
                &[
                    wgpu::BindingResource::TextureView(layers_view),
                    wgpu::BindingResource::TextureView(masks_view),
                    ops_buffer.as_entire_binding(),
                    jobs_buffer.as_entire_binding(),
                    wgpu::BindingResource::TextureView(target),
                    params_buffer.as_entire_binding(),
                    sources.curves.buffer.as_entire_binding(),
                    wgpu::BindingResource::TextureView(filtered_view),
                ],
            );
            Self::dispatch(encoder, &self.composite, &group, tiles.len() as u32);
            Ok(())
        };

        // Filter stages, in program order (docs/decisions/0023).
        let mut filtered: HashMap<(NodeId, TileCoord), u32> = HashMap::new();
        let passes = &sources.filters;
        for stage in &batch.stages {
            let region: Vec<(u32, &[Op], TileCoord)> = stage
                .region
                .iter()
                .enumerate()
                .map(|(slot, (coord, ops))| (slot as u32, ops.as_slice(), *coord))
                .collect();
            composite(encoder, &region, &filtered, &passes.stage.pool.view)?;
            let stage_pages = filter::page_table(
                &region
                    .iter()
                    .map(|(slot, _, coord)| (*coord, *slot))
                    .collect::<Vec<_>>(),
            );
            let rows: Vec<(TileCoord, u32)> = stage
                .rows
                .iter()
                .enumerate()
                .map(|(slot, coord)| (*coord, slot as u32))
                .collect();
            let first = filtered.len() as u32;
            let need: Vec<(TileCoord, u32)> = stage
                .need
                .iter()
                .enumerate()
                .map(|(index, coord)| (*coord, first + index as u32))
                .collect();
            let words = filter::params(document, stage, batch.level);
            let weights = filter::kernel(filter::sigma(&stage.filter, batch.level), stage.margin);
            let params_buffer = self.storage("cygnus filter parameters", &words);
            let weights_buffer =
                self.storage_bytes("cygnus filter weights", bytemuck::cast_slice(&weights));
            let jobs = |tiles: &[(TileCoord, u32)]| -> wgpu::Buffer {
                let words: Vec<u32> = tiles
                    .iter()
                    .flat_map(|(coord, slot)| [*slot, coord.tx, coord.ty, 0])
                    .collect();
                self.storage("cygnus filter jobs", &words)
            };
            let stage_pages_buffer = self.storage_bytes(
                "cygnus filter stage pages",
                bytemuck::cast_slice(&stage_pages),
            );
            let rows_jobs = jobs(&rows);
            let horizontal = self.bind(
                &passes.blur_h,
                &[
                    wgpu::BindingResource::TextureView(&passes.stage.pool.view),
                    stage_pages_buffer.as_entire_binding(),
                    weights_buffer.as_entire_binding(),
                    rows_jobs.as_entire_binding(),
                    wgpu::BindingResource::TextureView(&passes.rows.pool.view),
                    params_buffer.as_entire_binding(),
                ],
            );
            Self::dispatch(encoder, &passes.blur_h, &horizontal, rows.len() as u32);
            let rows_pages = self.storage_bytes(
                "cygnus filter rows pages",
                bytemuck::cast_slice(&filter::page_table(&rows)),
            );
            let need_jobs = jobs(&need);
            let vertical = self.bind(
                &passes.blur_v,
                &[
                    wgpu::BindingResource::TextureView(&passes.rows.pool.view),
                    rows_pages.as_entire_binding(),
                    weights_buffer.as_entire_binding(),
                    need_jobs.as_entire_binding(),
                    wgpu::BindingResource::TextureView(&passes.filtered.pool.view),
                    params_buffer.as_entire_binding(),
                    wgpu::BindingResource::TextureView(&passes.stage.pool.view),
                    stage_pages_buffer.as_entire_binding(),
                ],
            );
            Self::dispatch(encoder, &passes.blur_v, &vertical, need.len() as u32);
            for (coord, slot) in need {
                filtered.insert((stage.id, coord), slot);
            }
        }

        let outputs: Vec<(u32, &[Op], TileCoord)> = batch
            .outputs
            .iter()
            .map(|tile| (tile.target, tile.ops.as_slice(), tile.coord))
            .collect();
        composite(encoder, &outputs, &filtered, target)
    }

    fn export_batch(
        &self,
        document: &Document,
        batch: &Batch,
        image: &mut Image,
    ) -> Result<(), RenderError> {
        let scope = self.device.push_error_scope(wgpu::ErrorFilter::Validation);
        let memory = self.device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);

        let bits = image.bits.bits();
        // One u32 word per pixel in 8 bits, two in 16 bits.
        let words_per_tile = TILE_PIXELS * bits / 8;
        let mut export_jobs: Vec<u32> = Vec::new();
        for tile in &batch.outputs {
            export_jobs.extend([tile.target, tile.target * words_per_tile, bits, 0]);
        }
        let output_bytes = u64::from(words_per_tile) * batch.outputs.len() as u64 * 4;
        let export_buffer = self.storage("cygnus export jobs", &export_jobs);
        let output = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cygnus export output"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cygnus readback"),
            size: output_bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let export_group = self.bind(
            &self.export,
            &[
                wgpu::BindingResource::TextureView(&self.composites.view),
                export_buffer.as_entire_binding(),
                output.as_entire_binding(),
            ],
        );

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        self.record_composite(&mut encoder, document, batch, &self.composites.view)?;
        Self::dispatch(
            &mut encoder,
            &self.export,
            &export_group,
            batch.outputs.len() as u32,
        );
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, output_bytes);
        self.queue.submit([encoder.finish()]);

        if let Some(error) = pollster::block_on(memory.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }

        let words = self.read(&readback)?;
        for tile in &batch.outputs {
            let start = (tile.target * words_per_tile) as usize;
            unpack_tile(
                &words[start..start + words_per_tile as usize],
                tile.coord,
                image,
            );
        }
        Ok(())
    }

    pub(crate) fn read(&self, buffer: &wgpu::Buffer) -> Result<Vec<u32>, RenderError> {
        let slice = buffer.slice(..);
        let (sender, receiver) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            // The receiver outlives the poll below; a send error is harmless.
            let _ = sender.send(result);
        });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| RenderError::Gpu(error.to_string()))?;
        receiver
            .recv()
            .map_err(|error| RenderError::Gpu(error.to_string()))?
            .map_err(|error| RenderError::Gpu(error.to_string()))?;
        let view = slice
            .get_mapped_range()
            .map_err(|error| RenderError::Gpu(format!("{error:?}")))?;
        let words = bytemuck::cast_slice::<u8, u32>(&view).to_vec();
        drop(view);
        buffer.unmap();
        Ok(words)
    }
}

/// Smallest storage buffer bound to a shader: bindings may not be empty,
/// and must hold at least one structure even when there is no job (32
/// bytes: one composite operation).
const MIN_STORAGE_BYTES: usize = 32;

/// `bytes` extended with zeros to [`MIN_STORAGE_BYTES`] when shorter; the
/// content itself is always kept.
fn padded(bytes: &[u8]) -> std::borrow::Cow<'_, [u8]> {
    if bytes.len() < MIN_STORAGE_BYTES {
        let mut padded = bytes.to_vec();
        padded.resize(MIN_STORAGE_BYTES, 0);
        std::borrow::Cow::Owned(padded)
    } else {
        std::borrow::Cow::Borrowed(bytes)
    }
}

/// Copies the exported words of the tile at `coord` into `image`, cropping
/// what lies outside the document.
fn unpack_tile(words: &[u32], coord: TileCoord, image: &mut Image) {
    let area = coord.rect();
    let x_end = (area.x + area.width).min(image.width);
    let y_end = (area.y + area.height).min(image.height);
    for y in area.y..y_end {
        for x in area.x..x_end {
            let pixel = ((y - area.y) * TILE_SIZE + (x - area.x)) as usize;
            let rgba = match image.bits {
                ExportBits::Eight => {
                    let w = words[pixel];
                    [w & 0xFF, (w >> 8) & 0xFF, (w >> 16) & 0xFF, w >> 24]
                }
                ExportBits::Sixteen => {
                    let (rg, ba) = (words[2 * pixel], words[2 * pixel + 1]);
                    [rg & 0xFFFF, rg >> 16, ba & 0xFFFF, ba >> 16]
                }
            };
            let i = ((y * image.width + x) * 4) as usize;
            for (channel, value) in rgba.into_iter().enumerate() {
                image.data[i + channel] = value as u16;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_slots_follow_the_budget_and_the_array_layers() {
        const MIB: u64 = 1024 * 1024;
        // 512 MiB: 409.6 MiB of 512 KiB layer tiles, 102.4 MiB of 128 KiB
        // mask tiles.
        assert_eq!(source_slots(512 * MIB, 2048), (819, 819));
        assert_eq!(source_slots(256 * MIB, 2048), (409, 409));
        // The WebGPU default of 256 array layers bounds both pools.
        assert_eq!(source_slots(512 * MIB, 256), (256, 256));
        assert_eq!(source_slots(MIB, 2048), (16, 16));
    }

    #[test]
    fn short_storage_data_is_padded_not_replaced() {
        // A single 16-byte job: its words must survive (regression of B3,
        // where short data was replaced by zeros).
        let job: Vec<u8> = (1..=16).collect();
        let bytes = padded(&job);
        assert_eq!(bytes.len(), MIN_STORAGE_BYTES);
        assert_eq!(&bytes[..16], &job[..]);
        assert!(bytes[16..].iter().all(|b| *b == 0));
    }

    #[test]
    fn empty_storage_data_gives_zeros() {
        assert_eq!(padded(&[]).as_ref(), &[0u8; MIN_STORAGE_BYTES][..]);
    }

    #[test]
    fn long_enough_storage_data_is_kept_as_is() {
        for len in [MIN_STORAGE_BYTES, MIN_STORAGE_BYTES + 4, 4096] {
            let data: Vec<u8> = (0..len).map(|i| i as u8).collect();
            let bytes = padded(&data);
            assert!(matches!(bytes, std::borrow::Cow::Borrowed(_)));
            assert_eq!(bytes.as_ref(), &data[..]);
        }
    }
}

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

//! The interactive canvas: a bounded cache of composite tiles organized as
//! a pyramid, and the display pass that draws them (docs/decisions/0012 and
//! 0018).
//!
//! Level 0 holds composite tiles at full resolution; level L is level
//! L - 1 reduced by two, built from its four children. Only the tiles of
//! the displayed level that are visible stay pinned; the others are evicted
//! least recently used first when the cache is full. Tiles are rebuilt on
//! demand, depth first, so that building a coarse tile needs few slots at
//! once. When even the visible tiles do not fit, a coarser level is shown.
//!
//! The canvas knows nothing about windows: [`Canvas::prepare`] updates the
//! cache, [`Canvas::paint`] draws into any render pass.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, Instant};

use cygnus_photo_core::pixels::{TILE_SIZE, TileCoord};
use cygnus_photo_core::{Document, PhotoChanges};

use crate::format::COMPUTE_TEXEL_BYTES;
use crate::renderer::{Pool, RenderError, Renderer, pipeline};
use crate::shaders;

/// How the document is placed on the target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasView {
    /// Target position of the document origin, in pixels. Rounded to whole
    /// pixels by the caller, so that 100 % maps pixels exactly.
    pub origin: [f32; 2],
    /// Target pixels per document pixel (1.0 is 100 %).
    pub scale: f32,
    /// Visible area of the target: x, y, width, height in pixels.
    pub viewport: [f32; 4],
    /// sRGB color around the document.
    pub background: [f32; 3],
    /// sRGB colors of the transparency checker.
    pub checker: [[f32; 3]; 2],
    /// Side of a checker square, in target pixels.
    pub checker_size: f32,
}

/// Work counters, for tests and the benchmark.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CanvasStats {
    /// Tiles composited from layers: level 0, or a coarser level from
    /// reduced sources.
    pub composited: u64,
    /// Tiles of levels above 0 built from their children.
    pub downsampled: u64,
    /// Tiles evicted to make room.
    pub evicted: u64,
}

/// Work allowed per frame (docs/decisions/0024 and 0034, T1). The canvas
/// processes its queue of tiles while the processor time spent stays under
/// `cpu`, the estimated GPU time under `gpu` and the tiles under
/// `max_tiles`; the rest waits for the next frame. At least one batch is
/// processed per frame unless `max_tiles` is 0, which pauses the work (for
/// tests).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameBudget {
    /// Processor time spent recording and uploading.
    pub cpu: Duration,
    /// GPU time estimated from the measured cost of a tile.
    pub gpu: Duration,
    /// Hard bound on the tiles composited per frame.
    pub max_tiles: u32,
}

impl FrameBudget {
    /// Budget of the interactive canvas: 6 ms of processor time, 6 ms of
    /// estimated GPU time, 64 tiles.
    pub const DEFAULT: Self = Self {
        cpu: Duration::from_millis(6),
        gpu: Duration::from_millis(6),
        max_tiles: 64,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TileKey {
    level: u32,
    tx: u32,
    ty: u32,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    slot: u32,
    /// The content is that of the current document.
    valid: bool,
    /// The content is a complete image of the tile, possibly of an older
    /// document or approximate: it may be shown until it is replaced.
    shown: bool,
    used: u64,
    /// Built from reduced sources: approximate, replaced by the exact tile
    /// once no preview asks for it.
    approx: bool,
}

/// GPU work of one update, in dependency order: any prefix of a plan can
/// be run alone.
#[derive(Debug, Clone, Copy)]
enum Op {
    /// Composition of a tile: at level 0, or at a coarser level from
    /// reduced sources (approximate).
    Composite { key: TileKey, slot: u32 },
    Downsample {
        key: TileKey,
        parent: u32,
        children: [Option<u32>; 4],
        approx: bool,
    },
}

impl Op {
    fn key(&self) -> TileKey {
        match self {
            Self::Composite { key, .. } | Self::Downsample { key, .. } => *key,
        }
    }

    fn slot(&self) -> u32 {
        match self {
            Self::Composite { slot, .. } => *slot,
            Self::Downsample { parent, .. } => *parent,
        }
    }

    fn approx(&self) -> bool {
        match self {
            Self::Composite { key, .. } => key.level > 0,
            Self::Downsample { approx, .. } => *approx,
        }
    }
}

/// Source tiles uploaded at most per idle frame by the prefetch.
const PREFETCH_UPLOADS: usize = 16;

/// The tiles needed do not fit in the cache.
#[derive(Debug)]
struct OverBudget {
    /// Whether the cache was changed before the problem was found, in which
    /// case its content may be inconsistent and must be dropped.
    partial: bool,
}

const PARTIAL: OverBudget = OverBudget { partial: true };

/// Slots of the scratch pool: parents built in one downsample dispatch.
const SCRATCH_SLOTS: u32 = 16;
/// Smallest cache accepted when memory is short.
const MIN_SLOTS: u32 = 16;
/// Cache of a new canvas, before the document and the window are known.
const INITIAL_SLOTS: u32 = 64;
/// Levels described by the page table header (a 65 535-pixel document has
/// 9 levels).
const MAX_LEVELS: u32 = 16;
/// Words of the page table header: the number of levels, then for each
/// level the offset of its table, its tiles per row and its size.
const HEADER_WORDS: usize = 1 + 4 * MAX_LEVELS as usize;
/// GPU cost of a composite tile assumed before any measure, in ms.
const INITIAL_TILE_COST_MS: f64 = 0.5;
/// Weight of a new measure in the running average of the tile cost.
const COST_SMOOTHING: f64 = 0.25;

/// Number of tiles of all pyramid levels of a document of `size` pixels.
fn pyramid_tiles(size: (u32, u32)) -> (u64, u32) {
    let (mut width, mut height) = size;
    let mut tiles = 0u64;
    let mut levels = 0;
    loop {
        let (x, y) = (width.div_ceil(TILE_SIZE), height.div_ceil(TILE_SIZE));
        tiles += u64::from(x) * u64::from(y);
        if x <= 1 && y <= 1 {
            return (tiles, levels);
        }
        width = width.div_ceil(2);
        height = height.div_ceil(2);
        levels += 1;
    }
}

/// Cache size wanted for a document of `document` pixels shown in a window
/// of `viewport` pixels (docs/decisions/0018): the whole pyramid of the
/// document when possible, and at least twice the tiles a full window shows
/// at 100 %, plus one chain of children per level. Bounded afterwards by
/// the GPU (array layers, memory).
pub fn desired_slots(document: (u32, u32), viewport: (f32, f32)) -> u32 {
    let (pyramid, levels) = pyramid_tiles(document);
    let side = |length: f32| u64::from((length.max(0.0) / TILE_SIZE as f32).ceil() as u32 + 3);
    let window = 2 * side(viewport.0) * side(viewport.1) + 4 * u64::from(levels) + 1;
    u32::try_from(pyramid.max(window)).unwrap_or(u32::MAX)
}

/// Allocates a cache pool of `wanted` tiles, halving on GPU out-of-memory
/// down to `minimum`.
fn allocate_cache(
    device: &wgpu::Device,
    wanted: u32,
    minimum: u32,
) -> Result<(Pool, u32), RenderError> {
    allocate_with_retry(wanted, minimum, |n| {
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let pool = Pool::with_usage(
            device,
            "cygnus canvas cache",
            n,
            wgpu::TextureUsages::COPY_DST,
        );
        match pollster::block_on(memory.pop()) {
            Some(error) => Err(RenderError::Gpu(error.to_string())),
            None => Ok(pool),
        }
    })
}
const UNIFORM_BYTES: u64 = 80;

/// Calls `try_allocate` with `wanted` slots, halving on failure down to
/// `minimum`. Returns the pool and the number of slots obtained.
pub(crate) fn allocate_with_retry<T, E>(
    wanted: u32,
    minimum: u32,
    mut try_allocate: impl FnMut(u32) -> Result<T, E>,
) -> Result<(T, u32), E> {
    let mut slots = wanted.max(minimum);
    loop {
        match try_allocate(slots) {
            Ok(pool) => return Ok((pool, slots)),
            Err(error) if slots / 2 < minimum => return Err(error),
            Err(_) => slots /= 2,
        }
    }
}

/// Running estimate of the GPU time of one composite tile, measured from
/// the end of the submitted work (`on_submitted_work_done`).
struct GpuCost {
    per_tile_ms: f64,
    pending: Option<Measure>,
}

/// A measure in flight: when the work was submitted, how many tiles it
/// held, and when the GPU finished it.
type Measure = (Instant, f64, Arc<Mutex<Option<Instant>>>);

impl GpuCost {
    fn new() -> Self {
        Self {
            per_tile_ms: INITIAL_TILE_COST_MS,
            pending: None,
        }
    }

    /// Takes in the measure in flight if the GPU has finished it.
    fn update(&mut self, device: &wgpu::Device) {
        let Some((submitted, tiles, done)) = &self.pending else {
            return;
        };
        // Non-blocking: only runs the callbacks of finished work.
        let _ = device.poll(wgpu::PollType::Poll);
        let finished = *done.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(finished) = finished {
            let measured = finished.duration_since(*submitted).as_secs_f64() * 1000.0 / tiles;
            self.per_tile_ms += COST_SMOOTHING * (measured - self.per_tile_ms);
            self.pending = None;
        }
    }

    /// Starts a measure of the work submitted so far, holding `tiles`.
    fn measure(&mut self, queue: &wgpu::Queue, tiles: f64) {
        if self.pending.is_some() || tiles < 1.0 {
            return;
        }
        let done = Arc::new(Mutex::new(None));
        let flag = Arc::clone(&done);
        queue.on_submitted_work_done(move || {
            *flag.lock().unwrap_or_else(PoisonError::into_inner) = Some(Instant::now());
        });
        self.pending = Some((Instant::now(), tiles, done));
    }
}

/// What one frame may still spend.
struct Spending {
    budget: Option<FrameBudget>,
    start: Instant,
    gpu_ms: f64,
    tiles: f64,
    per_tile_ms: f64,
}

impl Spending {
    /// Composite tiles the next batch may hold (0: stop).
    fn allowance(&self, first: bool) -> usize {
        let Some(budget) = self.budget else {
            return usize::MAX;
        };
        if budget.max_tiles == 0 {
            return 0;
        }
        let tiles_left = f64::from(budget.max_tiles) - self.tiles;
        let gpu_left = (budget.gpu.as_secs_f64() * 1000.0 - self.gpu_ms) / self.per_tile_ms;
        let cpu_left = budget.cpu.saturating_sub(self.start.elapsed());
        let allowed = tiles_left.min(gpu_left).floor();
        if first {
            // Some progress every frame.
            return allowed.max(1.0) as usize;
        }
        if cpu_left.is_zero() || allowed < 1.0 {
            0
        } else {
            allowed as usize
        }
    }

    fn spend(&mut self, tiles: f64) {
        self.tiles += tiles;
        self.gpu_ms += tiles * self.per_tile_ms;
    }
}

/// Composite tile cache and display pipeline.
pub struct Canvas {
    display: wgpu::RenderPipeline,
    downsample: wgpu::ComputePipeline,
    cache: Pool,
    scratch: Pool,
    capacity: u32,
    /// Whether the cache size follows the document and the window.
    automatic: bool,
    /// Largest cache the GPU allows: array layers, lowered when an
    /// allocation runs out of memory.
    ceiling: u32,
    pages: wgpu::Buffer,
    pages_capacity: u64,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    entries: HashMap<TileKey, Entry>,
    slots: Vec<Option<TileKey>>,
    /// Tiles planned by the current prepare, and whether they are
    /// approximate.
    planned: HashMap<TileKey, bool>,
    clock: u64,
    size: (u32, u32),
    level: u32,
    /// The last prepare showed a coarser level than the zoom asks for.
    coarse: bool,
    /// A parameter is being dragged: compose from reduced sources.
    preview: bool,
    /// Work allowed per frame; `None` finishes every prepare.
    budget: Option<FrameBudget>,
    /// The last prepare left work for the next frames.
    pending: bool,
    gpu_cost: GpuCost,
    stats: CanvasStats,
}

impl Canvas {
    /// Creates the canvas for targets of `format` (egui's frame format).
    /// Its cache size follows the document and the window
    /// ([`desired_slots`]), within the array layers of the GPU and
    /// `memory_budget` bytes (see [`crate::gpu::cache_memory_budget`]). It
    /// works within [`FrameBudget::DEFAULT`] per frame.
    pub fn new(
        renderer: &Renderer,
        format: wgpu::TextureFormat,
        memory_budget: u64,
    ) -> Result<Self, RenderError> {
        let mut canvas = Self::build(renderer, format, INITIAL_SLOTS)?;
        canvas.automatic = true;
        canvas.budget = Some(FrameBudget::DEFAULT);
        let by_memory = memory_budget / u64::from(TILE_SIZE * TILE_SIZE * COMPUTE_TEXEL_BYTES);
        canvas.ceiling = canvas
            .ceiling
            .min(u32::try_from(by_memory).unwrap_or(u32::MAX))
            .max(MIN_SLOTS);
        Ok(canvas)
    }

    /// Creates the canvas with a fixed cache of at most `slots` tiles (16
    /// at least) and no frame budget, for tests. If the GPU is out of
    /// memory, the cache is halved until the allocation succeeds.
    pub fn with_capacity(
        renderer: &Renderer,
        format: wgpu::TextureFormat,
        slots: u32,
    ) -> Result<Self, RenderError> {
        Self::build(renderer, format, slots)
    }

    fn build(
        renderer: &Renderer,
        format: wgpu::TextureFormat,
        slots: u32,
    ) -> Result<Self, RenderError> {
        let device = renderer.device();
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cygnus display"),
            source: wgpu::ShaderSource::Wgsl(shaders::display().into()),
        });
        let display = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("cygnus display"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs_main"),
                targets: &[Some(format.into())],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let downsample = pipeline(device, "cygnus downsample", shaders::downsample());
        let ceiling = device.limits().max_texture_array_layers;
        let (cache, capacity) = allocate_cache(device, slots.min(ceiling), MIN_SLOTS)?;
        let scratch = Pool::with_usage(
            device,
            "cygnus canvas scratch",
            SCRATCH_SLOTS,
            wgpu::TextureUsages::COPY_SRC,
        );
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cygnus canvas view"),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pages_capacity = (HEADER_WORDS as u64 + 64).next_power_of_two();
        let pages = Self::pages_buffer(device, pages_capacity);
        let group = Self::bind_group(device, &display, &cache, &pages, &uniform);
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        Ok(Self {
            display,
            downsample,
            cache,
            scratch,
            capacity,
            automatic: false,
            ceiling,
            pages,
            pages_capacity,
            uniform,
            group,
            entries: HashMap::new(),
            slots: vec![None; capacity as usize],
            planned: HashMap::new(),
            clock: 0,
            size: (0, 0),
            level: 0,
            coarse: false,
            preview: false,
            budget: None,
            pending: false,
            gpu_cost: GpuCost::new(),
            stats: CanvasStats::default(),
        })
    }

    fn pages_buffer(device: &wgpu::Device, entries: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cygnus canvas pages"),
            size: entries * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    fn bind_group(
        device: &wgpu::Device,
        display: &wgpu::RenderPipeline,
        cache: &Pool,
        pages: &wgpu::Buffer,
        uniform: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cygnus display"),
            layout: &display.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&cache.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: pages.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: uniform.as_entire_binding(),
                },
            ],
        })
    }

    /// Resizes the cache to what the document and the window need, within
    /// the GPU limits. The cache content is dropped when it is resized.
    fn fit_capacity(
        &mut self,
        device: &wgpu::Device,
        viewport: (f32, f32),
        resized: bool,
    ) -> Result<(), RenderError> {
        if !self.automatic {
            return Ok(());
        }
        let desired = desired_slots(self.size, viewport).clamp(MIN_SLOTS, self.ceiling);
        let grow = desired > self.capacity;
        let shrink = resized && desired < self.capacity / 2;
        if !grow && !shrink {
            return Ok(());
        }
        let (cache, capacity) =
            allocate_cache(device, desired, MIN_SLOTS.max(self.capacity.min(desired)))?;
        if capacity < desired {
            // The GPU memory does not allow more: do not try again.
            self.ceiling = capacity;
        }
        self.cache = cache;
        self.capacity = capacity;
        self.entries.clear();
        self.slots = vec![None; capacity as usize];
        self.group = Self::bind_group(
            device,
            &self.display,
            &self.cache,
            &self.pages,
            &self.uniform,
        );
        Ok(())
    }

    /// Number of tiles the cache holds.
    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Sets the work allowed per frame; `None` finishes every prepare.
    pub fn set_frame_budget(&mut self, budget: Option<FrameBudget>) {
        self.budget = budget;
    }

    /// Whether the last [`Self::prepare`] left work for the next frames:
    /// the caller should draw another frame soon.
    pub fn is_pending(&self) -> bool {
        self.pending
    }

    /// Turns the preview on while a parameter is dragged (docs/decisions/
    /// 0027 and 0034): the displayed level, up to
    /// [`crate::preview::MAX_PREVIEW_LEVEL`], is composed from reduced
    /// sources; the tiles made so are recomputed exactly once the preview
    /// is turned off.
    pub fn set_preview(&mut self, on: bool) {
        self.preview = on;
    }

    /// Whether the last [`Self::prepare`] showed a coarser level than the
    /// zoom asks for, the cache being too small for the visible tiles.
    pub fn is_coarse(&self) -> bool {
        self.coarse
    }

    /// Cache slots of the tiles of the displayed level shown by the last
    /// [`Self::prepare`].
    pub fn shown_slots(&self) -> Vec<u32> {
        self.entries
            .iter()
            .filter(|(key, entry)| key.level == self.level && entry.shown)
            .map(|(_, entry)| entry.slot)
            .collect()
    }

    pub(crate) fn cache_view(&self) -> &wgpu::TextureView {
        &self.cache.view
    }

    /// Pyramid level shown by the last [`Self::prepare`] (0 is full size).
    pub fn level(&self) -> u32 {
        self.level
    }

    /// Work done since the canvas was created.
    pub fn stats(&self) -> CanvasStats {
        self.stats
    }

    /// Size of level `level` of the document, in its pixels.
    fn level_size(&self, level: u32) -> (u32, u32) {
        let factor = 1u32 << level;
        (self.size.0.div_ceil(factor), self.size.1.div_ceil(factor))
    }

    /// Tiles per row and column of level `level`.
    fn level_tiles(&self, level: u32) -> (u32, u32) {
        let (width, height) = self.level_size(level);
        (width.div_ceil(TILE_SIZE), height.div_ceil(TILE_SIZE))
    }

    /// Coarsest level: the whole document in one tile.
    fn top_level(&self) -> u32 {
        let mut level = 0;
        while self.level_tiles(level) != (1, 1) {
            level += 1;
        }
        level
    }

    /// Invalidates what `changes` touched; returns whether the document size
    /// changed. Invalid tiles stay shown until they are rebuilt.
    fn invalidate(&mut self, changes: &PhotoChanges, document: &Document) -> bool {
        let resized = self.size != (document.width, document.height);
        if resized {
            self.size = (document.width, document.height);
            self.entries.clear();
            self.slots.iter_mut().for_each(|slot| *slot = None);
            return true;
        }
        if changes.everything
            || changes.document
            || changes.structure
            || !changes.properties.is_empty()
        {
            for entry in self.entries.values_mut() {
                entry.valid = false;
            }
            return false;
        }
        for coord in changes.pixels.values().flatten() {
            // A level 0 tile changes every tile above it.
            for entry in self.entries.iter_mut() {
                let (key, entry) = entry;
                if key.tx == coord.tx >> key.level && key.ty == coord.ty >> key.level {
                    entry.valid = false;
                }
            }
        }
        false
    }

    /// Visible tiles of `level`, with one tile of margin for filtering.
    fn visible(&self, level: u32, view: &CanvasView) -> Vec<TileKey> {
        let (tiles_x, tiles_y) = self.level_tiles(level);
        let tile_px = view.scale * (TILE_SIZE << level) as f32;
        let [x, y, width, height] = view.viewport;
        let range = |start: f32, length: f32, origin: f32, count: u32| {
            let first = ((start - origin) / tile_px).floor() - 1.0;
            let last = ((start + length - origin) / tile_px).floor() + 1.0;
            let clamp = |v: f32| v.clamp(0.0, count.saturating_sub(1) as f32) as u32;
            (clamp(first), clamp(last))
        };
        let (x0, x1) = range(x, width, view.origin[0], tiles_x);
        let (y0, y1) = range(y, height, view.origin[1], tiles_y);
        let mut keys = Vec::new();
        for ty in y0..=y1 {
            for tx in x0..=x1 {
                keys.push(TileKey { level, tx, ty });
            }
        }
        keys
    }

    fn allocate(&mut self, key: TileKey, pinned: &HashSet<TileKey>) -> Result<u32, OverBudget> {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.used = self.clock;
            return Ok(entry.slot);
        }
        let slot = match self.slots.iter().position(Option::is_none) {
            Some(free) => free as u32,
            None => {
                let victim = self
                    .entries
                    .iter()
                    .filter(|(key, _)| !pinned.contains(key))
                    // Level 0 first: a tile of level L may cost up to 4^L
                    // compositions to rebuild. Then least recently used.
                    .min_by_key(|(key, entry)| (key.level, entry.used))
                    .map(|(key, entry)| (*key, entry.slot))
                    .ok_or(PARTIAL)?;
                self.entries.remove(&victim.0);
                // A planned tile evicted after its parent used it: its slot
                // is rewritten later in the same plan.
                self.planned.remove(&victim.0);
                self.stats.evicted += 1;
                victim.1
            }
        };
        self.slots[slot as usize] = Some(key);
        self.entries.insert(
            key,
            Entry {
                slot,
                valid: false,
                shown: false,
                used: self.clock,
                approx: false,
            },
        );
        Ok(slot)
    }

    /// Plans what makes `key` valid, appending the work to `ops`, and
    /// returns its slot and whether it is approximate; `None` when it
    /// cannot be built this frame (reduced sources not ready). Tiles of
    /// `approx_level` (0: none) are composed from reduced sources, and
    /// approximate tiles are accepted. Children are pinned only while their
    /// parent is planned.
    fn ensure(
        &mut self,
        key: TileKey,
        approx_level: u32,
        ready: &mut dyn FnMut(TileKey) -> bool,
        pinned: &mut HashSet<TileKey>,
        ops: &mut Vec<Op>,
    ) -> Result<Option<(u32, bool)>, OverBudget> {
        if let Some(approx) = self.planned.get(&key).copied() {
            return Ok(self.entries.get(&key).map(|entry| (entry.slot, approx)));
        }
        if let Some(entry) = self.entries.get_mut(&key)
            && entry.valid
            && (approx_level > 0 || !entry.approx)
        {
            entry.used = self.clock;
            return Ok(Some((entry.slot, entry.approx)));
        }
        let composite = key.level == 0 || (approx_level > 0 && key.level == approx_level);
        if composite && key.level > 0 && !ready(key) {
            return Ok(None);
        }
        let slot = self.allocate(key, pinned)?;
        // Pinned while its children are planned, so that they cannot take
        // its slot; the caller releases it.
        pinned.insert(key);
        let approx = if composite {
            ops.push(Op::Composite { key, slot });
            key.level > 0
        } else {
            let (tiles_x, tiles_y) = self.level_tiles(key.level - 1);
            let mut children = [None; 4];
            let mut held = Vec::new();
            let mut any_approx = false;
            let mut complete = true;
            for (quadrant, child) in children.iter_mut().enumerate() {
                let tx = key.tx * 2 + (quadrant as u32 % 2);
                let ty = key.ty * 2 + (quadrant as u32 / 2);
                if tx < tiles_x && ty < tiles_y {
                    let child_key = TileKey {
                        level: key.level - 1,
                        tx,
                        ty,
                    };
                    let child_was_pinned = pinned.contains(&child_key);
                    match self.ensure(child_key, approx_level, ready, pinned, ops)? {
                        Some((child_slot, child_approx)) => {
                            *child = Some(child_slot);
                            any_approx |= child_approx;
                        }
                        None => complete = false,
                    }
                    if !child_was_pinned {
                        held.push(child_key);
                    }
                }
            }
            for child in held {
                pinned.remove(&child);
            }
            if !complete {
                return Ok(None);
            }
            ops.push(Op::Downsample {
                key,
                parent: slot,
                children,
                approx: any_approx,
            });
            any_approx
        };
        self.planned.insert(key, approx);
        Ok(Some((slot, approx)))
    }

    /// Plans the work to show `level`: visible tiles pinned, then planned,
    /// approximate ones first when `approx_level` is set. Returns the work
    /// and whether some tile could not be planned this frame.
    fn plan_level(
        &mut self,
        level: u32,
        view: &CanvasView,
        approx_level: u32,
        exact: bool,
        ready: &mut dyn FnMut(TileKey) -> bool,
    ) -> Result<(Vec<Op>, bool), OverBudget> {
        let visible = self.visible(level, view);
        // Building a coarse tile keeps one chain of children per level.
        if visible.len() as u64 + 4 * u64::from(level) + 1 > u64::from(self.capacity) {
            return Err(OverBudget { partial: false });
        }
        self.planned.clear();
        // Visible tiles, resident or not, are never evicted.
        let mut pinned: HashSet<TileKey> = visible.iter().copied().collect();
        let mut ops = Vec::new();
        let mut waiting = false;
        if approx_level > 0 {
            for key in &visible {
                waiting |= self
                    .ensure(*key, approx_level, ready, &mut pinned, &mut ops)?
                    .is_none();
            }
        }
        if exact {
            // Approximate tiles planned above are rebuilt exactly.
            self.planned.retain(|_, approx| !*approx);
            for key in &visible {
                waiting |= self
                    .ensure(*key, 0, ready, &mut pinned, &mut ops)?
                    .is_none();
            }
        }
        Ok((ops, waiting))
    }

    /// Records and submits a prefix of `ops` within the frame budget and
    /// returns its length. Each composite batch is submitted before the
    /// next one makes its sources resident, so that a reused source slot is
    /// never read by older work.
    fn run(
        &mut self,
        renderer: &Renderer,
        document: &Document,
        ops: &[Op],
    ) -> Result<usize, RenderError> {
        let device = renderer.device();
        let queue = renderer.queue();
        self.gpu_cost.update(device);
        let mut spending = Spending {
            budget: self.budget,
            start: Instant::now(),
            gpu_ms: 0.0,
            tiles: 0.0,
            per_tile_ms: self.gpu_cost.per_tile_ms.max(0.01),
        };
        let new_encoder =
            || device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let mut encoder = new_encoder();
        let mut index = 0;
        while index < ops.len() {
            let allowed = spending.allowance(index == 0);
            if allowed == 0 {
                break;
            }
            match ops[index] {
                Op::Composite { key, .. } => {
                    let level = key.level;
                    let mut targets = HashMap::new();
                    while targets.len() < allowed
                        && let Some(Op::Composite { key, slot }) = ops.get(index)
                        && key.level == level
                    {
                        targets.insert(
                            TileCoord {
                                tx: key.tx,
                                ty: key.ty,
                            },
                            *slot,
                        );
                        index += 1;
                    }
                    let mut coords: Vec<TileCoord> = targets.keys().copied().collect();
                    coords.sort();
                    let batches = renderer.plan_at(
                        document,
                        coords,
                        usize::MAX,
                        |_, coord| targets.get(&coord).copied().unwrap_or(0),
                        level,
                    )?;
                    for batch in &batches {
                        queue.submit([std::mem::replace(&mut encoder, new_encoder()).finish()]);
                        renderer.record_composite(
                            &mut encoder,
                            document,
                            batch,
                            &self.cache.view,
                        )?;
                    }
                    self.stats.composited += targets.len() as u64;
                    spending.spend(targets.len() as f64);
                }
                Op::Downsample { .. } => {
                    // Parents built in one dispatch must not read each other.
                    let mut group: Vec<(u32, [Option<u32>; 4])> = Vec::new();
                    while let Some(Op::Downsample {
                        parent, children, ..
                    }) = ops.get(index)
                    {
                        let depends = children
                            .iter()
                            .flatten()
                            .any(|child| group.iter().any(|(p, _)| p == child));
                        if depends || group.len() == SCRATCH_SLOTS as usize {
                            break;
                        }
                        group.push((*parent, *children));
                        index += 1;
                    }
                    self.record_downsample(renderer, &mut encoder, &group);
                    self.stats.downsampled += group.len() as u64;
                    // A downsample reads four tiles and writes one: about a
                    // quarter of a composition.
                    spending.spend(group.len() as f64 / 4.0);
                }
            }
        }
        queue.submit([encoder.finish()]);
        self.gpu_cost.measure(queue, spending.tiles);
        Ok(index)
    }

    fn record_downsample(
        &self,
        renderer: &Renderer,
        encoder: &mut wgpu::CommandEncoder,
        group: &[(u32, [Option<u32>; 4])],
    ) {
        let mut jobs: Vec<u32> = Vec::new();
        for (scratch, (_, children)) in group.iter().enumerate() {
            for (quadrant, child) in children.iter().enumerate() {
                let child = child.map_or(-1i32, |slot| slot as i32);
                jobs.extend([child as u32, scratch as u32, quadrant as u32, 0]);
            }
        }
        let jobs_buffer = renderer.storage("cygnus downsample jobs", &jobs);
        let bind = renderer.bind(
            &self.downsample,
            &[
                wgpu::BindingResource::TextureView(&self.cache.view),
                jobs_buffer.as_entire_binding(),
                wgpu::BindingResource::TextureView(&self.scratch.view),
            ],
        );
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.downsample);
            pass.set_bind_group(0, &bind, &[]);
            let half = TILE_SIZE / 2 / 16;
            pass.dispatch_workgroups(half, half, jobs.len() as u32 / 4);
        }
        for (scratch, (parent, _)) in group.iter().enumerate() {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.scratch.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: scratch as u32,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &self.cache.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: *parent,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::Extent3d {
                    width: TILE_SIZE,
                    height: TILE_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    /// Brings the cache up to date with `document`, given what changed
    /// since the previous call, and sets the view. Within a frame budget,
    /// part of the work may be left for the next calls
    /// ([`Self::is_pending`]); meanwhile, older or coarser tiles are shown.
    pub fn prepare(
        &mut self,
        renderer: &Renderer,
        document: &Document,
        changes: &PhotoChanges,
        view: &CanvasView,
    ) -> Result<(), RenderError> {
        let resized = self.invalidate(changes, document);
        self.fit_capacity(
            renderer.device(),
            (view.viewport[2], view.viewport[3]),
            resized,
        )?;
        self.clock += 1;

        // The level whose pixels are between one and two screen pixels.
        let top = self.top_level();
        let mut level = 0;
        while level < top && view.scale * ((2u32 << level) as f32) <= 1.0 {
            level += 1;
        }
        let wanted = level;
        let blocking = self.budget.is_none();
        let mut ready = |key: TileKey| {
            renderer.reduced_ready(
                document,
                TileCoord {
                    tx: key.tx,
                    ty: key.ty,
                },
                key.level,
                blocking,
            )
        };
        let (ops, waiting) = loop {
            let approx_level = if self.preview {
                level.min(crate::preview::MAX_PREVIEW_LEVEL)
            } else {
                0
            };
            match self.plan_level(level, view, approx_level, !self.preview, &mut ready) {
                Ok(plan) => break plan,
                Err(OverBudget { partial }) if level < top => {
                    // Not enough room: go coarser. Only a plan that failed
                    // halfway leaves the cache to be dropped; the usual case
                    // (too many visible tiles) is detected before any change.
                    if partial {
                        self.entries.clear();
                        self.slots.iter_mut().for_each(|slot| *slot = None);
                    }
                    level += 1;
                }
                Err(OverBudget { .. }) => {
                    return Err(RenderError::Unsupported(
                        "a canvas cache this small for this view",
                    ));
                }
            }
        };
        self.coarse = level > wanted;
        self.level = level;

        let device = renderer.device();
        let queue = renderer.queue();
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);

        let done = if ops.is_empty() {
            0
        } else {
            self.run(renderer, document, &ops)?
        };
        for op in &ops[..done] {
            // A tile evicted later in the plan has lost its slot.
            if let Some(entry) = self.entries.get_mut(&op.key())
                && entry.slot == op.slot()
            {
                entry.valid = true;
                entry.shown = true;
                entry.approx = op.approx();
            }
        }
        self.planned.clear();
        self.pending = done < ops.len() || waiting;
        if ops.is_empty() && !waiting && level == 0 && !self.preview {
            // Idle at full size: bring back the sources of the visible
            // tiles, so that a property change uploads none (0027).
            let coords: Vec<TileCoord> = self
                .visible(0, view)
                .into_iter()
                .map(|key| TileCoord {
                    tx: key.tx,
                    ty: key.ty,
                })
                .collect();
            renderer.prefetch(document, &coords, PREFETCH_UPLOADS)?;
        }

        self.write_pages(device, queue, top);
        queue.write_buffer(&self.uniform, 0, &self.uniform_bytes(view, level));

        if let Some(error) = pollster::block_on(memory.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        Ok(())
    }

    /// Writes the page tables of every level (docs/decisions/0034, T2): the
    /// display falls back on a coarser level where a tile is not shown.
    fn write_pages(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, top: u32) {
        let levels = (top + 1).min(MAX_LEVELS);
        let mut words = vec![0i32; HEADER_WORDS];
        words[0] = levels as i32;
        let mut offset = HEADER_WORDS;
        for level in 0..levels {
            let (tiles_x, tiles_y) = self.level_tiles(level);
            let (width, height) = self.level_size(level);
            let base = 1 + 4 * level as usize;
            words[base] = offset as i32;
            words[base + 1] = tiles_x as i32;
            words[base + 2] = width as i32;
            words[base + 3] = height as i32;
            offset += (tiles_x * tiles_y) as usize;
        }
        words.resize(offset, -1);
        for (key, entry) in &self.entries {
            if key.level < levels && entry.shown {
                let base = 1 + 4 * key.level as usize;
                let index =
                    words[base] as usize + (key.ty * words[base + 1] as u32 + key.tx) as usize;
                if let Some(word) = words.get_mut(index) {
                    *word = entry.slot as i32;
                }
            }
        }
        if words.len() as u64 > self.pages_capacity {
            self.pages_capacity = (words.len() as u64).next_power_of_two();
            self.pages = Self::pages_buffer(device, self.pages_capacity);
            self.group = Self::bind_group(
                device,
                &self.display,
                &self.cache,
                &self.pages,
                &self.uniform,
            );
        }
        queue.write_buffer(&self.pages, 0, bytemuck::cast_slice(&words));
    }

    fn uniform_bytes(&self, view: &CanvasView, level: u32) -> Vec<u8> {
        let (width, height) = self.level_size(level);
        let mut words: Vec<u32> = vec![
            view.origin[0].to_bits(),
            view.origin[1].to_bits(),
            view.scale.to_bits(),
            ((1u32 << level) as f32).to_bits(),
            width,
            height,
            level,
            view.checker_size.max(1.0).to_bits(),
        ];
        for color in [view.background, view.checker[0], view.checker[1]] {
            words.extend(color.map(f32::to_bits));
            words.push(1f32.to_bits());
        }
        bytemuck::cast_slice(&words).to_vec()
    }

    /// Draws the canvas over the whole viewport of `pass`.
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.display);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_holds_the_whole_pyramid_of_a_24_megapixel_document() {
        // 384 + 96 + 24 + 6 + 2 + 1 tiles over six levels.
        assert_eq!(pyramid_tiles((6000, 4000)), (513, 5));
        assert_eq!(desired_slots((6000, 4000), (2126.0, 1086.0)), 513);
    }

    #[test]
    fn budget_follows_a_4k_window_for_small_documents() {
        // 3840 × 2160: (15 + 3) × (9 + 3) tiles, twice, for a one-tile document.
        assert_eq!(desired_slots((200, 200), (3840.0, 2160.0)), 2 * 18 * 12 + 1);
    }

    #[test]
    fn allocation_retries_with_half_the_slots_when_out_of_memory() {
        // Simulated GPU memory: allocations above 64 slots fail.
        let mut attempts = Vec::new();
        let (pool, slots) = allocate_with_retry(256, 16, |n| {
            attempts.push(n);
            if n > 64 { Err("out of memory") } else { Ok(n) }
        })
        .unwrap();
        assert_eq!((pool, slots), (64, 64));
        assert_eq!(attempts, vec![256, 128, 64]);
    }

    #[test]
    fn allocation_gives_up_below_the_minimum() {
        let result = allocate_with_retry(256, 16, |_| Err::<u32, _>("out of memory"));
        assert_eq!(result, Err("out of memory"));
    }
}

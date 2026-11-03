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
    /// Level 0 tiles composited from layers.
    pub composited: u64,
    /// Tiles of levels above 0 built from their children.
    pub downsampled: u64,
    /// Tiles evicted to make room.
    pub evicted: u64,
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
    valid: bool,
    used: u64,
    /// Built from reduced sources during a preview: valid only while the
    /// preview lasts, then recomputed exactly.
    preview: bool,
}

/// GPU work of one update, in dependency order.
enum Op {
    /// Composition of a tile of `level`: 0, or the preview level (from
    /// reduced sources).
    Composite {
        coord: TileCoord,
        slot: u32,
        level: u32,
    },
    Downsample {
        parent: u32,
        children: [Option<u32>; 4],
    },
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
    clock: u64,
    size: (u32, u32),
    level: u32,
    /// The last prepare showed a coarser level than the zoom asks for.
    coarse: bool,
    /// A parameter is being dragged: compose from reduced sources.
    preview: bool,
    /// Level composed from reduced sources by the current prepare (0: none).
    preview_level: u32,
    stats: CanvasStats,
}

impl Canvas {
    /// Creates the canvas for targets of `format` (egui's frame format).
    /// Its cache size follows the document and the window
    /// ([`desired_slots`]), within the array layers of the GPU and
    /// `memory_budget` bytes (see [`crate::gpu::cache_memory_budget`]).
    pub fn new(
        renderer: &Renderer,
        format: wgpu::TextureFormat,
        memory_budget: u64,
    ) -> Result<Self, RenderError> {
        let mut canvas = Self::build(renderer, format, INITIAL_SLOTS)?;
        canvas.automatic = true;
        let by_memory = memory_budget / u64::from(TILE_SIZE * TILE_SIZE * COMPUTE_TEXEL_BYTES);
        canvas.ceiling = canvas
            .ceiling
            .min(u32::try_from(by_memory).unwrap_or(u32::MAX))
            .max(MIN_SLOTS);
        Ok(canvas)
    }

    /// Creates the canvas with a fixed cache of at most `slots` tiles (16
    /// at least), for tests. If the GPU is out of memory, the cache is
    /// halved until the allocation succeeds.
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
        let pages_capacity = 64;
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
            clock: 0,
            size: (0, 0),
            level: 0,
            coarse: false,
            preview: false,
            preview_level: 0,
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

    /// Turns the preview on while a parameter is dragged (docs/decisions/
    /// 0027, B7-lite): the displayed level, up to level 2, is composed from
    /// reduced sources; the tiles made so are recomputed exactly once the
    /// preview is turned off.
    pub fn set_preview(&mut self, on: bool) {
        self.preview = on;
    }

    /// Whether the last [`Self::prepare`] showed a coarser level than the
    /// zoom asks for, the cache being too small for the visible tiles.
    pub fn is_coarse(&self) -> bool {
        self.coarse
    }

    /// Cache slots of the tiles shown by the last [`Self::prepare`].
    pub fn shown_slots(&self) -> Vec<u32> {
        self.entries
            .iter()
            .filter(|(key, entry)| key.level == self.level && entry.valid)
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
    /// changed.
    fn invalidate(&mut self, changes: &PhotoChanges, document: &Document) -> bool {
        let resized = self.size != (document.width, document.height);
        if resized
            || changes.everything
            || changes.document
            || changes.structure
            || !changes.properties.is_empty()
        {
            self.size = (document.width, document.height);
            for entry in self.entries.values_mut() {
                entry.valid = false;
            }
            return resized;
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
                used: self.clock,
                preview: false,
            },
        );
        Ok(slot)
    }

    /// Makes `key` resident and valid, appending the work to `ops`. Children
    /// are pinned only while their parent is built.
    fn ensure(
        &mut self,
        key: TileKey,
        pinned: &mut HashSet<TileKey>,
        ops: &mut Vec<Op>,
    ) -> Result<u32, OverBudget> {
        let previewing = self.preview_level > 0;
        if let Some(entry) = self.entries.get_mut(&key)
            && entry.valid
            && (previewing || !entry.preview)
        {
            entry.used = self.clock;
            return Ok(entry.slot);
        }
        let slot = self.allocate(key, pinned)?;
        // Pinned while its children are built, so that they cannot take
        // its slot; the caller releases it.
        pinned.insert(key);
        let mut preview = false;
        if key.level == 0 || (previewing && key.level == self.preview_level) {
            ops.push(Op::Composite {
                coord: TileCoord {
                    tx: key.tx,
                    ty: key.ty,
                },
                slot,
                level: key.level,
            });
            preview = key.level > 0;
            self.stats.composited += 1;
        } else {
            let (tiles_x, tiles_y) = self.level_tiles(key.level - 1);
            let mut children = [None; 4];
            let mut held = Vec::new();
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
                    *child = Some(self.ensure(child_key, pinned, ops)?);
                    preview |= self.entries.get(&child_key).is_some_and(|e| e.preview);
                    if !child_was_pinned {
                        held.push(child_key);
                    }
                }
            }
            ops.push(Op::Downsample {
                parent: slot,
                children,
            });
            self.stats.downsampled += 1;
            for child in held {
                pinned.remove(&child);
            }
        }
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.valid = true;
            entry.preview = preview;
        }
        Ok(slot)
    }

    /// Plans the work to show `level`: visible tiles pinned, then built.
    fn plan_level(&mut self, level: u32, view: &CanvasView) -> Result<Vec<Op>, OverBudget> {
        let visible = self.visible(level, view);
        // Building a coarse tile keeps one chain of children per level.
        if visible.len() as u64 + 4 * u64::from(level) + 1 > u64::from(self.capacity) {
            return Err(OverBudget { partial: false });
        }
        // Visible tiles, resident or not, are never evicted.
        let mut pinned: HashSet<TileKey> = visible.iter().copied().collect();
        let mut ops = Vec::new();
        for key in visible {
            self.ensure(key, &mut pinned, &mut ops)?;
        }
        Ok(ops)
    }

    /// Records and submits the work of `ops`. Each composite batch is
    /// submitted before the next one makes its sources resident, so that
    /// a reused source slot is never read by older work.
    fn run(&self, renderer: &Renderer, document: &Document, ops: &[Op]) -> Result<(), RenderError> {
        let device = renderer.device();
        let queue = renderer.queue();
        let new_encoder =
            || device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        let mut encoder = new_encoder();
        let mut index = 0;
        while index < ops.len() {
            match &ops[index] {
                Op::Composite { level, .. } => {
                    let level = *level;
                    let mut targets = HashMap::new();
                    while let Some(Op::Composite {
                        coord,
                        slot,
                        level: same,
                    }) = ops.get(index)
                        && *same == level
                    {
                        targets.insert(*coord, *slot);
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
                }
                Op::Downsample { .. } => {
                    // Parents built in one dispatch must not read each other.
                    let mut group: Vec<(u32, [Option<u32>; 4])> = Vec::new();
                    while let Some(Op::Downsample { parent, children }) = ops.get(index) {
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
                }
            }
        }
        queue.submit([encoder.finish()]);
        Ok(())
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
    /// since the previous call, and sets the view.
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
        let ops = loop {
            self.preview_level = if self.preview {
                level.min(crate::preview::MAX_PREVIEW_LEVEL)
            } else {
                0
            };
            match self.plan_level(level, view) {
                Ok(ops) => break ops,
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

        if !ops.is_empty() {
            self.run(renderer, document, &ops)?;
        } else if level == 0 && !self.preview {
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

        let (tiles_x, tiles_y) = self.level_tiles(level);
        let count = u64::from(tiles_x) * u64::from(tiles_y);
        if count > self.pages_capacity {
            self.pages_capacity = count.next_power_of_two();
            self.pages = Self::pages_buffer(device, self.pages_capacity);
            self.group = Self::bind_group(
                device,
                &self.display,
                &self.cache,
                &self.pages,
                &self.uniform,
            );
        }
        let mut pages = vec![-1i32; count as usize];
        for (key, entry) in &self.entries {
            if key.level == level && entry.valid {
                pages[(key.ty * tiles_x + key.tx) as usize] = entry.slot as i32;
            }
        }
        queue.write_buffer(&self.pages, 0, bytemuck::cast_slice(&pages));
        queue.write_buffer(&self.uniform, 0, &self.uniform_bytes(view, level));

        if let Some(error) = pollster::block_on(memory.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        Ok(())
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
            self.level_tiles(level).0,
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

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

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError, mpsc};

use cygnus_photo_core::pixels::{TILE_SIZE, Tile, TileCoord, tiles_in};
use cygnus_photo_core::{BlendMode, Document};

use crate::format::COMPUTE_FORMAT;
use crate::image::{ExportBits, Image};
use crate::program::{self, Op};
use crate::shaders;
use crate::sources::{PoolKind, SourceKey, SourceStats, TilePool};

/// Slots of each source pool by default, within the array layers of the
/// device: 128 MiB of layer tiles and 32 MiB of mask tiles.
const DEFAULT_SOURCE_SLOTS: u32 = 256;
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
    /// A layer uses a blend mode the renderer does not support yet.
    #[error("blend mode {0:?} is not supported yet")]
    UnsupportedBlendMode(BlendMode),
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
    pub(crate) outputs: Vec<OutputTile>,
    layers: BTreeSet<SourceKey>,
    masks: BTreeSet<SourceKey>,
}

impl Batch {
    fn new() -> Self {
        Self {
            outputs: Vec::new(),
            layers: BTreeSet::new(),
            masks: BTreeSet::new(),
        }
    }
}

impl Renderer {
    /// Creates the pipelines and pools on `device`, with source pools of
    /// [`DEFAULT_SOURCE_SLOTS`] tiles each (fewer if the device has fewer
    /// array layers).
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, RenderError> {
        let slots = DEFAULT_SOURCE_SLOTS.min(device.limits().max_texture_array_layers);
        Self::with_source_slots(device, queue, slots, slots)
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

    /// Counters of the source pools.
    pub fn source_stats(&self) -> SourceStats {
        self.sources
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .stats
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
        mut target: impl FnMut(usize, TileCoord) -> u32,
    ) -> Result<Vec<Batch>, RenderError> {
        let capacity = self.source_stats().capacity;
        let (layer_slots, mask_slots) = (capacity.0 as usize, capacity.1 as usize);
        let mut batches = Vec::new();
        let mut batch = Batch::new();
        for coord in outputs {
            let ops = program::tile_program(document, coord)?;
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
                batches.push(std::mem::replace(&mut batch, Batch::new()));
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
        // Bindings may not be empty, and must hold at least one structure
        // even when there is no job (32 bytes: one composite operation). The buffer is written through
        // the queue, not mapped at creation: wgpu's mapped initialization
        // panics when the GPU is out of memory, while a queue write reports
        // the error to the error scopes like any other.
        const MIN_BYTES: usize = 32;
        let mut padded;
        let bytes = if bytes.len() < MIN_BYTES {
            padded = bytes.to_vec();
            padded.resize(MIN_BYTES, 0);
            &padded[..]
        } else {
            bytes
        };
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
                        document
                            .pixels
                            .tile(key.0, key.1)
                            .map(|tile| (*key, tile))
                            .ok_or(RenderError::Unsupported(
                                "a source tile missing from its document",
                            ))
                    })
                    .collect()
            };
        let layer_slots = sources.layers.ensure(
            &self.queue,
            &tiles(&batch.layers)?,
            sources.clock,
            &mut sources.stats,
        )?;
        let mask_slots = sources.masks.ensure(
            &self.queue,
            &tiles(&batch.masks)?,
            sources.clock,
            &mut sources.stats,
        )?;

        // One job per output tile, eight words per operation.
        let mut ops: Vec<u32> = Vec::new();
        let mut jobs: Vec<u32> = Vec::new();
        for tile in &batch.outputs {
            jobs.extend([
                (ops.len() / OP_WORDS) as u32,
                tile.ops.len() as u32,
                tile.target,
                0,
            ]);
            for op in &tile.ops {
                let slot = op
                    .source
                    .and_then(|key| layer_slots.get(&key).copied())
                    .unwrap_or(0);
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
                    u32::from(op.mask.inverted),
                ]);
            }
        }
        let ops_buffer = self.storage("cygnus composite ops", &ops);
        let jobs_buffer = self.storage("cygnus composite jobs", &jobs);
        let group = self.bind(
            &self.composite,
            &[
                wgpu::BindingResource::TextureView(&sources.layers.view),
                wgpu::BindingResource::TextureView(&sources.masks.view),
                ops_buffer.as_entire_binding(),
                jobs_buffer.as_entire_binding(),
                wgpu::BindingResource::TextureView(target),
            ],
        );
        Self::dispatch(encoder, &self.composite, &group, batch.outputs.len() as u32);
        Ok(())
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

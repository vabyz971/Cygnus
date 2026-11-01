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

//! Tile renderer: decodes layer tiles, composites them and exports the
//! result (docs/decisions/0011 and 0012).
//!
//! The document is processed in batches of output tiles, so that it never
//! has to be resident on the GPU at once. Each batch decodes the layer
//! tiles it needs into the layer pool, runs the composition program of each
//! output tile into the composite pool, then converts the composites to
//! integers for the readback. Every GPU error is caught by an error scope
//! and returned, never turned into a crash.

use std::collections::BTreeMap;
use std::sync::mpsc;

use cygnus_photo_core::pixels::{TILE_SIZE, Tile, TileCoord, tiles_in};
use cygnus_photo_core::{BlendMode, Document, NodeId, NodeKind};

use crate::format::COMPUTE_FORMAT;
use crate::image::{ExportBits, Image};
use crate::shaders;

/// Slots of the layer pool: decoded layer tiles of one batch.
const LAYER_SLOTS: u32 = 128;
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
    /// A structure the renderer does not support yet (groups, J3).
    #[error("{0} is not supported yet")]
    Unsupported(&'static str),
    /// A tile has more layers than the pool can hold at once.
    #[error("{count} layers overlap one tile, at most {LAYER_SLOTS} are supported")]
    TooManyLayers {
        /// Number of overlapping layers.
        count: usize,
    },
    /// The GPU reported an error.
    #[error("GPU error: {0}")]
    Gpu(String),
}

/// One layer of the composition program.
#[derive(Debug, Clone, Copy)]
struct ProgramLayer {
    id: NodeId,
    opacity: f32,
    mode: BlendMode,
}

/// Visible pixel layers, bottom first. Groups arrive at milestone J3.
fn program(document: &Document) -> Result<Vec<ProgramLayer>, RenderError> {
    let mut layers = Vec::new();
    for id in document.children(document.root) {
        let Some(node) = document.node(*id) else {
            continue;
        };
        if !matches!(node.kind, NodeKind::Pixel { .. }) {
            return Err(RenderError::Unsupported("rendering a group"));
        }
        if !node.blend.is_supported() {
            return Err(RenderError::UnsupportedBlendMode(node.blend));
        }
        if node.visible {
            layers.push(ProgramLayer {
                id: *id,
                opacity: node.opacity,
                mode: node.blend,
            });
        }
    }
    Ok(layers)
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

/// Tile renderer bound to one GPU device.
pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    decode: wgpu::ComputePipeline,
    composite: wgpu::ComputePipeline,
    export: wgpu::ComputePipeline,
    layers: Pool,
    composites: Pool,
}

/// One output tile of a batch: where it goes, and the layer tiles it
/// composites (with their slots in the layer pool).
pub(crate) struct OutputTile {
    pub(crate) coord: TileCoord,
    pub(crate) target: u32,
    ops: Vec<(ProgramLayer, u32)>,
}

/// Output tiles whose layer tiles fit in the layer pool together.
pub(crate) struct Batch {
    pub(crate) outputs: Vec<OutputTile>,
    slots: BTreeMap<(NodeId, TileCoord), u32>,
}

impl Renderer {
    /// Creates the pipelines and pools on `device`.
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Result<Self, RenderError> {
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let renderer = Self {
            device: device.clone(),
            queue: queue.clone(),
            decode: pipeline(device, "cygnus decode", shaders::decode()),
            composite: pipeline(device, "cygnus composite", shaders::composite()),
            export: pipeline(device, "cygnus export", shaders::export()),
            layers: Pool::new(device, "cygnus layer pool", LAYER_SLOTS),
            composites: Pool::new(device, "cygnus composite pool", COMPOSITE_SLOTS),
        };
        match pollster::block_on(scope.pop()) {
            Some(error) => Err(RenderError::Gpu(error.to_string())),
            None => Ok(renderer),
        }
    }

    pub(crate) fn device(&self) -> &wgpu::Device {
        &self.device
    }

    pub(crate) fn queue(&self) -> &wgpu::Queue {
        &self.queue
    }

    /// Splits the composition of `outputs` into batches whose layer tiles
    /// fit in the layer pool. `target(index_in_batch, coord)` gives the
    /// destination slot of each output tile; `max_outputs` bounds a batch.
    pub(crate) fn plan(
        &self,
        document: &Document,
        outputs: impl IntoIterator<Item = TileCoord>,
        max_outputs: usize,
        mut target: impl FnMut(usize, TileCoord) -> u32,
    ) -> Result<Vec<Batch>, RenderError> {
        let layers = program(document)?;
        let mut batches = Vec::new();
        let mut batch = Batch {
            outputs: Vec::new(),
            slots: BTreeMap::new(),
        };
        for coord in outputs {
            let present: Vec<ProgramLayer> = layers
                .iter()
                .copied()
                .filter(|layer| document.pixels.tile(layer.id, coord).is_some())
                .collect();
            if present.len() > LAYER_SLOTS as usize {
                return Err(RenderError::TooManyLayers {
                    count: present.len(),
                });
            }
            let full = batch.outputs.len() == max_outputs
                || batch.slots.len() + present.len() > LAYER_SLOTS as usize;
            if full {
                batches.push(std::mem::replace(
                    &mut batch,
                    Batch {
                        outputs: Vec::new(),
                        slots: BTreeMap::new(),
                    },
                ));
            }
            let mut ops = Vec::with_capacity(present.len());
            for layer in present {
                let next = batch.slots.len() as u32;
                let slot = *batch.slots.entry((layer.id, coord)).or_insert(next);
                ops.push((layer, slot));
            }
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
        // Bindings may not be empty, and must hold at least one 16-byte job
        // structure even when there is no job. The buffer is written through
        // the queue, not mapped at creation: wgpu's mapped initialization
        // panics when the GPU is out of memory, while a queue write reports
        // the error to the error scopes like any other.
        let padding = [0u8; 16];
        let bytes = if bytes.len() < padding.len() {
            &padding[..]
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

    /// Records the decode and composite passes of `batch` into `encoder`,
    /// writing each output tile to its target slot of `target`.
    pub(crate) fn record_composite(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        document: &Document,
        batch: &Batch,
        target: &wgpu::TextureView,
    ) {
        // Decode: raw tiles, packed in u32 words, to the layer pool.
        let mut raw: Vec<u8> = Vec::new();
        let mut decode_jobs: Vec<u32> = Vec::new();
        for ((layer, coord), slot) in &batch.slots {
            let Some(tile) = document.pixels.tile(*layer, *coord) else {
                continue;
            };
            let offset = (raw.len() / 4) as u32;
            let bits = pack_tile(tile, &mut raw);
            decode_jobs.extend([offset, *slot, bits, 0]);
        }
        // Composite: one job per output tile.
        let mut ops: Vec<u32> = Vec::new();
        let mut composite_jobs: Vec<u32> = Vec::new();
        for tile in &batch.outputs {
            composite_jobs.extend([ops.len() as u32 / 4, tile.ops.len() as u32, tile.target, 0]);
            for (layer, slot) in &tile.ops {
                ops.extend([*slot, layer.mode.code(), layer.opacity.to_bits(), 0]);
            }
        }
        let raw_buffer = self.storage_bytes("cygnus raw tiles", &raw);
        let decode_buffer = self.storage("cygnus decode jobs", &decode_jobs);
        let ops_buffer = self.storage("cygnus composite ops", &ops);
        let composite_buffer = self.storage("cygnus composite jobs", &composite_jobs);
        let decode_group = self.bind(
            &self.decode,
            &[
                raw_buffer.as_entire_binding(),
                decode_buffer.as_entire_binding(),
                wgpu::BindingResource::TextureView(&self.layers.view),
            ],
        );
        let composite_group = self.bind(
            &self.composite,
            &[
                wgpu::BindingResource::TextureView(&self.layers.view),
                ops_buffer.as_entire_binding(),
                composite_buffer.as_entire_binding(),
                wgpu::BindingResource::TextureView(target),
            ],
        );
        let decode_count = decode_jobs.len() as u32 / 4;
        if decode_count > 0 {
            Self::dispatch(encoder, &self.decode, &decode_group, decode_count);
        }
        Self::dispatch(
            encoder,
            &self.composite,
            &composite_group,
            batch.outputs.len() as u32,
        );
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
        self.record_composite(&mut encoder, document, batch, &self.composites.view);
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

/// Appends `tile` to `raw` in the layout the decode shader reads (u32
/// words: RGBA bytes in 8 bits, RG and BA pairs of u16 in 16 bits, little
/// endian), and returns its bits per channel. On little-endian machines this
/// is the memory layout of the tile itself: a plain copy.
fn pack_tile(tile: &Tile, raw: &mut Vec<u8>) -> u32 {
    #[cfg(target_endian = "little")]
    match tile {
        Tile::U8(values) => {
            raw.extend_from_slice(values);
            8
        }
        Tile::U16(values) => {
            raw.extend_from_slice(bytemuck::cast_slice(values));
            16
        }
    }
    #[cfg(not(target_endian = "little"))]
    match tile {
        Tile::U8(values) => {
            raw.extend_from_slice(values);
            8
        }
        Tile::U16(values) => {
            raw.extend(values.iter().flat_map(|value| value.to_le_bytes()));
            16
        }
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

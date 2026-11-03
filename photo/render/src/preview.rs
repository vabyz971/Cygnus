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

//! Reduced sources for the preview shown while a parameter is dragged
//! (docs/decisions/0027, B7-lite). A tile of pyramid level L (1 or 2) of a
//! layer or a mask is built on the GPU from the 2^L × 2^L source tiles it
//! covers, then kept resident while those tiles are unchanged: a parameter
//! drag changes no pixel, so after its first frame the canvas composes the
//! displayed level directly from small reduced tiles instead of
//! recomposing the whole full-size level.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use cygnus_photo_core::pixels::{TILE_SIZE, Tile, TileCoord};
use cygnus_photo_core::{Document, NodeId, NodeKind};

use crate::format::{MASK_FORMAT, MASK_TEXEL_BYTES, SOURCE_FORMAT, SOURCE_TEXEL_BYTES};
use crate::program::layer_tile;
use crate::renderer::RenderError;
use crate::sources::{PoolKind, SourceKey, SourceStats, TilePool};

/// Coarsest level composed from reduced sources.
pub const MAX_PREVIEW_LEVEL: u32 = 2;
/// Reduced tiles built per submission.
const JOBS_PER_SUBMIT: usize = 16;
/// Value of an absent child slot (`NO_CHILD` in WGSL).
const NO_CHILD: u32 = u32::MAX;

/// Identity of the source tiles a reduced tile was built from.
type Stamp = Vec<usize>;

struct Built {
    slot: u32,
    stamp: Stamp,
    used: u64,
}

/// A pool of reduced tiles of one kind, in a texture array of the same
/// format as the source pool of that kind.
pub(crate) struct ReducedPool {
    kind: PoolKind,
    texture: wgpu::Texture,
    pub(crate) view: wgpu::TextureView,
    capacity: u32,
    entries: HashMap<(SourceKey, u32), Built>,
    free: Vec<u32>,
}

impl ReducedPool {
    pub(crate) fn new(device: &wgpu::Device, kind: PoolKind, capacity: u32) -> Self {
        let format = match kind {
            PoolKind::Layers => SOURCE_FORMAT,
            PoolKind::Masks => MASK_FORMAT,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("cygnus reduced source pool"),
            size: wgpu::Extent3d {
                width: TILE_SIZE,
                height: TILE_SIZE,
                depth_or_array_layers: capacity,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        Self {
            kind,
            texture,
            view,
            capacity,
            entries: HashMap::new(),
            free: (0..capacity).rev().collect(),
        }
    }

    pub(crate) fn capacity(&self) -> u32 {
        self.capacity
    }

    fn texel_bytes(&self) -> u32 {
        match self.kind {
            PoolKind::Layers => SOURCE_TEXEL_BYTES,
            PoolKind::Masks => MASK_TEXEL_BYTES,
        }
    }
}

/// The source tiles covered by `key` at `level`, row by row (absent ones
/// `None`).
fn children(document: &Document, key: SourceKey, level: u32) -> Vec<Option<&Arc<Tile>>> {
    let factor = 1u32 << level;
    let mut out = Vec::with_capacity((factor * factor) as usize);
    for dy in 0..factor {
        for dx in 0..factor {
            let coord = TileCoord {
                tx: key.1.tx * factor + dx,
                ty: key.1.ty * factor + dy,
            };
            out.push(layer_tile(document, key.0, coord));
        }
    }
    out
}

fn stamp(tiles: &[Option<&Arc<Tile>>]) -> Stamp {
    tiles
        .iter()
        .map(|tile| tile.map_or(0, |tile| Arc::as_ptr(tile) as usize))
        .collect()
}

/// Empty value of the mask `id`, in 16-bit units (0 for layers).
fn empty_value(document: &Document, id: NodeId) -> u32 {
    match document.node(id).map(|node| &node.kind) {
        Some(NodeKind::Mask { empty, .. }) => {
            let max = match document.depth {
                cygnus_photo_core::Depth::U8 => 255,
                cygnus_photo_core::Depth::U16 => 65535,
            };
            u32::from(*empty) * 65535 / max
        }
        _ => 0,
    }
}

/// The two reduction pipelines.
pub(crate) struct Reducer {
    layers: wgpu::ComputePipeline,
    masks: wgpu::ComputePipeline,
}

impl Reducer {
    pub(crate) fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cygnus reduce"),
            source: wgpu::ShaderSource::Wgsl(crate::shaders::reduce().into()),
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
        Self {
            layers: pipeline("reduce_layers"),
            masks: pipeline("reduce_masks"),
        }
    }
}

/// Context of one call of [`ensure_reduced`].
pub(crate) struct Build<'a> {
    pub(crate) device: &'a wgpu::Device,
    pub(crate) queue: &'a wgpu::Queue,
    pub(crate) reducer: &'a Reducer,
    pub(crate) clock: u64,
}

/// Makes the reduced tiles `wanted` of `level` resident in `reduced`,
/// building the missing or stale ones from `sources` (the source pool of
/// the same kind), and returns their slots. Each group of builds is
/// submitted before the next one reuses source slots.
pub(crate) fn ensure_reduced(
    build: &Build<'_>,
    document: &Document,
    level: u32,
    wanted: &[SourceKey],
    reduced: &mut ReducedPool,
    sources: &mut TilePool,
    stats: &mut SourceStats,
) -> Result<HashMap<SourceKey, u32>, RenderError> {
    if wanted.len() > reduced.capacity as usize {
        return Err(RenderError::TooManyLayers {
            count: wanted.len(),
        });
    }
    let pinned: HashSet<(SourceKey, u32)> = wanted.iter().map(|key| (*key, level)).collect();
    let mut slots = HashMap::with_capacity(wanted.len());
    let mut to_build: Vec<(SourceKey, u32)> = Vec::new();
    for key in wanted {
        let tiles = children(document, *key, level);
        let current = stamp(&tiles);
        let entry_key = (*key, level);
        let slot = match reduced.entries.get_mut(&entry_key) {
            Some(entry) if entry.stamp == current => {
                entry.used = build.clock;
                entry.slot
            }
            Some(entry) => {
                entry.stamp = current;
                entry.used = build.clock;
                to_build.push((*key, entry.slot));
                entry.slot
            }
            None => {
                let slot = match reduced.free.pop() {
                    Some(slot) => slot,
                    None => {
                        let victim = reduced
                            .entries
                            .iter()
                            .filter(|(k, _)| !pinned.contains(k))
                            .min_by_key(|(k, entry)| (entry.used, k.0))
                            .map(|(k, _)| *k)
                            .ok_or(RenderError::TooManyLayers {
                                count: wanted.len(),
                            })?;
                        reduced
                            .entries
                            .remove(&victim)
                            .map(|entry| entry.slot)
                            .ok_or(RenderError::Unsupported("a reduced tile without slot"))?
                    }
                };
                reduced.entries.insert(
                    entry_key,
                    Built {
                        slot,
                        stamp: current,
                        used: build.clock,
                    },
                );
                to_build.push((*key, slot));
                slot
            }
        };
        slots.insert(*key, slot);
    }

    let factor = 1u32 << level;
    let per_tile = (factor * factor) as usize;
    // Children of one submission must fit in the source pool.
    let per_submit = JOBS_PER_SUBMIT.min((sources.capacity() as usize / per_tile).max(1));
    for chunk in to_build.chunks(per_submit) {
        let mut wanted_sources: Vec<(SourceKey, &Arc<Tile>)> = Vec::new();
        let mut child_lists = Vec::with_capacity(chunk.len());
        for (key, _) in chunk {
            let tiles = children(document, *key, level);
            let mut list = Vec::with_capacity(per_tile);
            for (index, tile) in tiles.iter().enumerate() {
                let coord = TileCoord {
                    tx: key.1.tx * factor + index as u32 % factor,
                    ty: key.1.ty * factor + index as u32 / factor,
                };
                if let Some(tile) = tile {
                    wanted_sources.push(((key.0, coord), tile));
                    list.push(Some((key.0, coord)));
                } else {
                    list.push(None);
                }
            }
            child_lists.push(list);
        }
        let source_slots = sources.ensure(build.queue, &wanted_sources, build.clock, stats)?;
        let tile_words = (TILE_SIZE * TILE_SIZE * reduced.texel_bytes() / 4) as usize;
        let mut jobs: Vec<u32> = Vec::with_capacity(chunk.len() * 4);
        let mut child_slots: Vec<u32> = Vec::with_capacity(chunk.len() * per_tile);
        for (index, ((key, _), list)) in chunk.iter().zip(&child_lists).enumerate() {
            jobs.extend([
                child_slots.len() as u32,
                empty_value(document, key.0),
                (index * tile_words) as u32,
                factor,
            ]);
            child_slots.extend(list.iter().map(|child| {
                child
                    .and_then(|child| source_slots.get(&child).copied())
                    .unwrap_or(NO_CHILD)
            }));
        }
        let storage = |label, words: &[u32]| {
            let buffer = build.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: (words.len().max(4) * 4) as u64,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            build
                .queue
                .write_buffer(&buffer, 0, bytemuck::cast_slice(words));
            buffer
        };
        let jobs_buffer = storage("cygnus reduce jobs", &jobs);
        let children_buffer = storage("cygnus reduce children", &child_slots);
        let output_bytes = (chunk.len() * tile_words * 4) as u64;
        let output = build.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cygnus reduce output"),
            size: output_bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let pipeline = match reduced.kind {
            PoolKind::Layers => &build.reducer.layers,
            PoolKind::Masks => &build.reducer.masks,
        };
        let group = build.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&sources.view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: jobs_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: children_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = build
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &group, &[]);
            let width = match reduced.kind {
                PoolKind::Layers => TILE_SIZE,
                PoolKind::Masks => TILE_SIZE / 2,
            };
            pass.dispatch_workgroups(width / 16, TILE_SIZE / 16, chunk.len() as u32);
        }
        for (index, (_, slot)) in chunk.iter().enumerate() {
            encoder.copy_buffer_to_texture(
                wgpu::TexelCopyBufferInfo {
                    buffer: &output,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: (index * tile_words * 4) as u64,
                        bytes_per_row: Some(TILE_SIZE * reduced.texel_bytes()),
                        rows_per_image: Some(TILE_SIZE),
                    },
                },
                wgpu::TexelCopyTextureInfo {
                    texture: &reduced.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: 0,
                        y: 0,
                        z: *slot,
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
        build.queue.submit([encoder.finish()]);
        stats.reduced += chunk.len() as u64;
    }
    Ok(slots)
}

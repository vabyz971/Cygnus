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

//! `brush.stroke` on the GPU (docs/decisions/0035 and 0036): the dabs
//! computed by `photo/core` are sorted by the tiles they reach, in their
//! order, and `brush.wgsl` paints each tile of the target in one pass. The
//! result, in the document depth, is read back as the new tiles of the
//! layer.

use std::collections::BTreeMap;
use std::sync::Arc;

use cygnus_photo_core::brush::{Brush, Dab};
use cygnus_photo_core::pixels::{TILE_SIZE, Tile, TileBackup, TileCoord, tiles_in};
use cygnus_photo_core::resolve::Paint;
use cygnus_photo_core::{Depth, Document, NodeId, Rect};

use crate::renderer::{RenderError, Renderer, pipeline};
use crate::shaders;

/// Tiles painted per dispatch at most.
const TILES_PER_BATCH: usize = 32;
/// Dab entries (a dab counts once per tile it reaches) per dispatch at
/// most.
const DABS_PER_BATCH: usize = 1 << 20;
/// Words of one tile: two per pixel.
const TILE_WORDS: usize = (TILE_SIZE * TILE_SIZE * 2) as usize;

/// The tiles of `bounds` each dab reaches, with the indices of the dabs in
/// order.
fn bins(dabs: &[Dab], bounds: Rect) -> BTreeMap<TileCoord, Vec<usize>> {
    let mut bins: BTreeMap<TileCoord, Vec<usize>> = BTreeMap::new();
    let right = f64::from(bounds.x) + f64::from(bounds.width);
    let bottom = f64::from(bounds.y) + f64::from(bounds.height);
    for (index, dab) in dabs.iter().enumerate() {
        let reach = dab.radius + 1.0;
        let x0 = (dab.x - reach).floor().max(f64::from(bounds.x));
        let y0 = (dab.y - reach).floor().max(f64::from(bounds.y));
        let x1 = (dab.x + reach).ceil().min(right);
        let y1 = (dab.y + reach).ceil().min(bottom);
        if x1 <= x0 || y1 <= y0 {
            continue;
        }
        let rect = Rect {
            x: x0 as u32,
            y: y0 as u32,
            width: (x1 - x0) as u32,
            height: (y1 - y0) as u32,
        };
        for coord in tiles_in(rect) {
            bins.entry(coord).or_default().push(index);
        }
    }
    bins
}

/// Straight values of `tile`, 16 bits (8-bit ones times 257), packed two
/// per word; zeros for a missing tile.
fn pack(tile: Option<&Arc<Tile>>, words: &mut Vec<u32>) {
    let Some(tile) = tile else {
        words.resize(words.len() + TILE_WORDS, 0);
        return;
    };
    let wide = |v: u16, eight: bool| u32::from(if eight { v * 257 } else { v });
    let (values, eight): (Vec<u16>, bool) = match tile.as_ref() {
        Tile::U8(values) => (values.iter().map(|v| u16::from(*v)).collect(), true),
        Tile::U16(values) => (values.to_vec(), false),
        // A pixel layer only holds RGBA tiles.
        Tile::Gray8(_) | Tile::Gray16(_) => {
            words.resize(words.len() + TILE_WORDS, 0);
            return;
        }
    };
    for pixel in values.as_chunks::<4>().0 {
        words.push(wide(pixel[0], eight) | wide(pixel[1], eight) << 16);
        words.push(wide(pixel[2], eight) | wide(pixel[3], eight) << 16);
    }
}

/// The tile of the painted `words`, in the document depth; `None` when it
/// is fully transparent (the empty value of a pixel layer).
fn unpack(words: &[u32], depth: Depth) -> Option<Arc<Tile>> {
    if words.iter().all(|word| *word == 0) {
        return None;
    }
    let values = words
        .iter()
        .flat_map(|word| [(word & 0xffff) as u16, (word >> 16) as u16]);
    Some(Arc::new(match depth {
        Depth::U8 => Tile::U8(values.map(|v| v as u8).collect()),
        Depth::U16 => Tile::U16(values.collect()),
    }))
}

impl Renderer {
    fn paint_tiles(
        &self,
        document: &Document,
        target: NodeId,
        brush: &Brush,
        dabs: &[Dab],
    ) -> Result<TileBackup, RenderError> {
        let bounds = cygnus_photo_core::commands::stroke_target(document, target)
            .map_err(|_| RenderError::Unsupported("painting other than an editable pixel layer"))?;
        let bins: Vec<(TileCoord, Vec<usize>)> = bins(dabs, bounds).into_iter().collect();
        let device = self.device();
        let pipeline = self
            .brush_pipeline
            .get_or_init(|| pipeline(device, "cygnus brush", shaders::brush()));
        let max_value = match document.depth {
            Depth::U8 => 255.0f32,
            Depth::U16 => 65535.0,
        };
        let params: Vec<u32> = brush
            .color
            .iter()
            .map(|c| c.to_bits())
            .chain([
                brush.opacity.to_bits(),
                brush.hardness.to_bits(),
                brush.mode.code(),
                max_value.to_bits(),
            ])
            .collect();
        let params = self.storage("cygnus brush parameters", &params);

        let mut painted = Vec::with_capacity(bins.len());
        let mut start = 0;
        while start < bins.len() {
            // A batch: up to TILES_PER_BATCH tiles and DABS_PER_BATCH dabs.
            let mut end = start;
            let mut entries = 0;
            while end < bins.len()
                && end - start < TILES_PER_BATCH
                && (end == start || entries + bins[end].1.len() <= DABS_PER_BATCH)
            {
                entries += bins[end].1.len();
                end += 1;
            }
            let batch = &bins[start..end];
            let mut jobs: Vec<u32> = Vec::with_capacity(batch.len() * 8);
            let mut dab_words: Vec<u32> = Vec::with_capacity(entries * 4);
            let mut input: Vec<u32> = Vec::with_capacity(batch.len() * TILE_WORDS);
            for (index, (coord, indices)) in batch.iter().enumerate() {
                let area = coord.rect().intersect(&bounds);
                let origin = (
                    f64::from(coord.tx * TILE_SIZE),
                    f64::from(coord.ty * TILE_SIZE),
                );
                jobs.extend([
                    (dab_words.len() / 4) as u32,
                    indices.len() as u32,
                    area.width,
                    area.height,
                    (index * TILE_WORDS) as u32,
                    0,
                    0,
                    0,
                ]);
                for dab in indices.iter().map(|i| dabs[*i]) {
                    // Relative to the tile origin, where f32 keeps a fine
                    // precision whatever the size of the document.
                    dab_words.extend(
                        [dab.x - origin.0, dab.y - origin.1, dab.radius, dab.flow]
                            .map(|v| (v as f32).to_bits()),
                    );
                }
                pack(document.pixels.tile(target, *coord), &mut input);
            }
            let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
            let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
            let jobs = self.storage("cygnus brush jobs", &jobs);
            let dab_buffer = self.storage("cygnus brush dabs", &dab_words);
            let input = self.storage("cygnus brush input", &input);
            let bytes = (batch.len() * TILE_WORDS * 4) as u64;
            let output = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cygnus brush output"),
                size: bytes,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            });
            let readback = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("cygnus brush readback"),
                size: bytes,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let group = self.bind(
                pipeline,
                &[
                    params.as_entire_binding(),
                    jobs.as_entire_binding(),
                    dab_buffer.as_entire_binding(),
                    input.as_entire_binding(),
                    output.as_entire_binding(),
                ],
            );
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            {
                let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
                pass.set_pipeline(pipeline);
                pass.set_bind_group(0, &group, &[]);
                pass.dispatch_workgroups(TILE_SIZE / 16, TILE_SIZE / 16, batch.len() as u32);
            }
            encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
            self.queue().submit([encoder.finish()]);
            if let Some(error) = pollster::block_on(memory.pop()) {
                return Err(RenderError::Gpu(error.to_string()));
            }
            if let Some(error) = pollster::block_on(scope.pop()) {
                return Err(RenderError::Gpu(error.to_string()));
            }
            let words = self.read(&readback)?;
            for (index, (coord, _)) in batch.iter().enumerate() {
                let tile = &words[index * TILE_WORDS..(index + 1) * TILE_WORDS];
                painted.push((*coord, unpack(tile, document.depth)));
            }
            start = end;
        }
        Ok(painted)
    }
}

impl Paint for Renderer {
    fn paint(
        &self,
        document: &Document,
        target: NodeId,
        brush: &Brush,
        dabs: &[Dab],
    ) -> Result<TileBackup, String> {
        self.paint_tiles(document, target, brush, dabs)
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dab(x: f64, y: f64, radius: f64) -> Dab {
        Dab {
            x,
            y,
            radius,
            flow: 1.0,
        }
    }

    #[test]
    fn dabs_are_binned_by_the_tiles_they_reach_in_order() {
        let bounds = Rect {
            x: 0,
            y: 0,
            width: 600,
            height: 300,
        };
        let bins = bins(
            &[
                dab(250.0, 10.0, 10.0),
                dab(100.0, 100.0, 5.0),
                dab(-50.0, 0.0, 5.0),
            ],
            bounds,
        );
        let first = TileCoord { tx: 0, ty: 0 };
        let second = TileCoord { tx: 1, ty: 0 };
        assert_eq!(bins.len(), 2);
        assert_eq!(bins[&first], vec![0, 1]);
        assert_eq!(bins[&second], vec![0]);
    }

    #[test]
    fn eight_bit_tiles_round_trip_through_the_words() {
        let values: Vec<u8> = (0..TILE_SIZE * TILE_SIZE * 4)
            .map(|i| (i % 256) as u8)
            .collect();
        let tile = Arc::new(Tile::U8(values.clone().into_boxed_slice()));
        let mut words = Vec::new();
        pack(Some(&tile), &mut words);
        assert_eq!(words[0], 257 << 16);
        assert_eq!(words[1] & 0xffff, 2 * 257);
        // The shader writes 8-bit values back for an 8-bit document.
        let narrow: Vec<u32> = values
            .as_chunks::<2>()
            .0
            .iter()
            .map(|pair| u32::from(pair[0]) | u32::from(pair[1]) << 16)
            .collect();
        assert_eq!(unpack(&narrow, Depth::U8).as_deref(), Some(tile.as_ref()));
        assert!(unpack(&vec![0; TILE_WORDS], Depth::U16).is_none());
    }
}

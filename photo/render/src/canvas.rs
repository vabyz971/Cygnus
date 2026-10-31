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

//! The interactive canvas: a cache of composite tiles and the display pass
//! that draws them (docs/decisions/0012).
//!
//! The canvas knows nothing about windows: [`Canvas::prepare`] updates the
//! tiles invalidated by the command changes, [`Canvas::paint`] draws into
//! any render pass. The application calls them from an egui-wgpu callback;
//! the tests call them on an offscreen texture.
//!
//! Before milestone step B7 (tile pyramid and memory budget), every tile of
//! the document stays resident at full resolution, which limits documents
//! to [`Canvas::MAX_TILES`] tiles.

use cygnus_photo_core::pixels::{TILE_SIZE, TileCoord, tiles_in};
use cygnus_photo_core::{Document, PhotoChanges};

use crate::renderer::{Pool, RenderError, Renderer};
use crate::shaders;

/// How the document is placed on the target.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasView {
    /// Target position of the document origin, in pixels. Rounded to whole
    /// pixels by the caller, so that 100 % maps pixels exactly.
    pub origin: [f32; 2],
    /// Target pixels per document pixel (1.0 is 100 %).
    pub scale: f32,
    /// sRGB color around the document.
    pub background: [f32; 3],
    /// sRGB colors of the transparency checker.
    pub checker: [[f32; 3]; 2],
    /// Side of a checker square, in target pixels.
    pub checker_size: f32,
}

/// Composite tile cache and display pipeline.
pub struct Canvas {
    pipeline: wgpu::RenderPipeline,
    cache: Pool,
    pages: wgpu::Buffer,
    uniform: wgpu::Buffer,
    group: wgpu::BindGroup,
    size: (u32, u32),
    valid: Vec<bool>,
}

const UNIFORM_BYTES: u64 = 80;

impl Canvas {
    /// Largest number of tiles of a displayed document until the tile
    /// pyramid arrives (B7): 256 tiles, about 16.7 Mpx.
    pub const MAX_TILES: u32 = 256;

    /// Creates the canvas for targets of `format` (egui's frame format).
    pub fn new(renderer: &Renderer, format: wgpu::TextureFormat) -> Result<Self, RenderError> {
        let device = renderer.device();
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("cygnus display"),
            source: wgpu::ShaderSource::Wgsl(shaders::display().into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
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
        let cache = Pool::new(device, "cygnus canvas cache", Self::MAX_TILES);
        let pages = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cygnus canvas pages"),
            size: u64::from(Self::MAX_TILES) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cygnus canvas view"),
            size: UNIFORM_BYTES,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("cygnus display"),
            layout: &pipeline.get_bind_group_layout(0),
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
        });
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        Ok(Self {
            pipeline,
            cache,
            pages,
            uniform,
            group,
            size: (0, 0),
            valid: Vec::new(),
        })
    }

    fn tiles_x(&self) -> u32 {
        self.size.0.div_ceil(TILE_SIZE)
    }

    fn index(&self, coord: TileCoord) -> usize {
        (coord.ty * self.tiles_x() + coord.tx) as usize
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
        let tiles = tiles_in(document.bounds()).count();
        if tiles > Self::MAX_TILES as usize {
            return Err(RenderError::Unsupported(
                "displaying documents above 256 tiles (about 16.7 Mpx)",
            ));
        }
        let resized = self.size != (document.width, document.height);
        if resized
            || changes.everything
            || changes.document
            || changes.structure
            || !changes.properties.is_empty()
        {
            self.size = (document.width, document.height);
            self.valid = vec![false; tiles];
        } else {
            for coord in changes.pixels.values().flatten() {
                let index = self.index(*coord);
                if let Some(valid) = self.valid.get_mut(index) {
                    *valid = false;
                }
            }
        }

        let device = renderer.device();
        let queue = renderer.queue();
        let scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
        let memory = device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);

        let stale: Vec<TileCoord> = tiles_in(document.bounds())
            .filter(|coord| !self.valid[self.index(*coord)])
            .collect();
        if !stale.is_empty() {
            let tiles_x = self.tiles_x();
            let batches =
                renderer.plan(document, stale.iter().copied(), usize::MAX, |_, coord| {
                    coord.ty * tiles_x + coord.tx
                })?;
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            for batch in &batches {
                renderer.record_composite(&mut encoder, document, batch, &self.cache.view);
            }
            queue.submit([encoder.finish()]);
            for coord in stale {
                let index = self.index(coord);
                self.valid[index] = true;
            }
        }

        // Every tile has its own slot: slot = index.
        let pages: Vec<i32> = (0..tiles as i32).collect();
        queue.write_buffer(&self.pages, 0, bytemuck::cast_slice(&pages));
        queue.write_buffer(&self.uniform, 0, &self.uniform_bytes(view));

        if let Some(error) = pollster::block_on(memory.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        if let Some(error) = pollster::block_on(scope.pop()) {
            return Err(RenderError::Gpu(error.to_string()));
        }
        Ok(())
    }

    fn uniform_bytes(&self, view: &CanvasView) -> Vec<u8> {
        let mut words: Vec<u32> = vec![
            view.origin[0].to_bits(),
            view.origin[1].to_bits(),
            view.scale.to_bits(),
            self.tiles_x(),
            self.size.0,
            self.size.1,
            view.checker_size.max(1.0).to_bits(),
            0,
        ];
        for color in [view.background, view.checker[0], view.checker[1]] {
            words.extend(color.map(f32::to_bits));
            words.push(1f32.to_bits());
        }
        bytemuck::cast_slice(&words).to_vec()
    }

    /// Draws the canvas over the whole viewport of `pass`.
    pub fn paint(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.group, &[]);
        pass.draw(0..3, 0..1);
    }
}

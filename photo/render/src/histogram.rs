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

//! Histogram of the image shown by the canvas (REG-4, docs/decisions/0025),
//! counted on the GPU over the composite tiles of the displayed level and
//! read back without blocking: [`HistogramPass::record`] adds the pass to
//! a frame, [`HistogramPass::poll`] returns the counts one or two frames
//! later. It describes the area shown, at the displayed pyramid level.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::canvas::Canvas;
use crate::renderer::{Renderer, pipeline};
use crate::shaders;

/// Words of the counts: 3 × 256 bins and the pixel count.
const WORDS: u64 = 3 * 256 + 1;

/// Counts of one histogram.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Histogram {
    /// Bins of red, green and blue, from 0 to 255.
    pub bins: [[u32; 256]; 3],
    /// Pixels counted (alpha above 0).
    pub pixels: u64,
}

impl Histogram {
    /// Mean and standard deviation of the three channels together, in 8-bit
    /// levels (0 to 255), from the bins.
    pub fn mean_and_std_dev(&self) -> (f64, f64) {
        let mut count = 0.0;
        let mut sum = 0.0;
        let mut squares = 0.0;
        for channel in &self.bins {
            for (level, n) in channel.iter().enumerate() {
                let (n, v) = (f64::from(*n), level as f64);
                count += n;
                sum += n * v;
                squares += n * v * v;
            }
        }
        if count == 0.0 {
            return (0.0, 0.0);
        }
        let mean = sum / count;
        (mean, (squares / count - mean * mean).max(0.0).sqrt())
    }
}

enum State {
    Idle,
    Recorded,
    Mapping(Arc<AtomicBool>),
}

/// The histogram pass and its readback buffers.
pub struct HistogramPass {
    pipeline: wgpu::ComputePipeline,
    counts: wgpu::Buffer,
    staging: wgpu::Buffer,
    state: State,
}

impl HistogramPass {
    /// Creates the pass on the device of `renderer`.
    pub fn new(renderer: &Renderer) -> Self {
        let device = renderer.device();
        let buffer = |label, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: WORDS * 4,
                usage,
                mapped_at_creation: false,
            })
        };
        Self {
            pipeline: pipeline(device, "cygnus histogram", shaders::histogram()),
            counts: buffer(
                "cygnus histogram counts",
                wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_SRC
                    | wgpu::BufferUsages::COPY_DST,
            ),
            staging: buffer(
                "cygnus histogram readback",
                wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            ),
            state: State::Idle,
        }
    }

    /// Whether a histogram can be recorded now (none is in flight).
    pub fn is_idle(&self) -> bool {
        matches!(self.state, State::Idle)
    }

    /// Records the counting of the tiles `canvas` shows into `encoder`, if
    /// no histogram is in flight. The encoder must be submitted after the
    /// canvas work of this frame.
    pub fn record(
        &mut self,
        renderer: &Renderer,
        canvas: &Canvas,
        encoder: &mut wgpu::CommandEncoder,
    ) {
        if !self.is_idle() {
            return;
        }
        let slots = canvas.shown_slots();
        encoder.clear_buffer(&self.counts, 0, None);
        if !slots.is_empty() {
            let slots_buffer = renderer.storage("cygnus histogram slots", &slots);
            let group = renderer.bind(
                &self.pipeline,
                &[
                    wgpu::BindingResource::TextureView(canvas.cache_view()),
                    slots_buffer.as_entire_binding(),
                    self.counts.as_entire_binding(),
                ],
            );
            let tile = cygnus_photo_core::pixels::TILE_SIZE / 16;
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &group, &[]);
            pass.dispatch_workgroups(tile, tile, slots.len() as u32);
        }
        encoder.copy_buffer_to_buffer(&self.counts, 0, &self.staging, 0, WORDS * 4);
        self.state = State::Recorded;
    }

    /// Advances the readback; returns the histogram once it is read. Call
    /// once per frame, after the frame that recorded it was submitted.
    pub fn poll(&mut self, device: &wgpu::Device) -> Option<Histogram> {
        match &self.state {
            State::Idle => None,
            State::Recorded => {
                let ready = Arc::new(AtomicBool::new(false));
                let flag = Arc::clone(&ready);
                self.staging
                    .slice(..)
                    .map_async(wgpu::MapMode::Read, move |result| {
                        if result.is_ok() {
                            flag.store(true, Ordering::Release);
                        }
                    });
                self.state = State::Mapping(ready);
                let _ = device.poll(wgpu::PollType::Poll);
                None
            }
            State::Mapping(ready) => {
                let _ = device.poll(wgpu::PollType::Poll);
                if !ready.load(Ordering::Acquire) {
                    return None;
                }
                let words: Option<Vec<u32>> = self
                    .staging
                    .slice(..)
                    .get_mapped_range()
                    .ok()
                    .map(|view| bytemuck::cast_slice(&view).to_vec());
                self.staging.unmap();
                self.state = State::Idle;
                let words = words?;
                let mut bins = [[0u32; 256]; 3];
                for (channel, bins) in bins.iter_mut().enumerate() {
                    bins.copy_from_slice(&words[channel * 256..(channel + 1) * 256]);
                }
                Some(Histogram {
                    bins,
                    pixels: u64::from(words[768]),
                })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_and_deviation_come_from_the_bins() {
        let mut bins = [[0u32; 256]; 3];
        // Half the values at 100, half at 200, on every channel.
        for channel in &mut bins {
            channel[100] = 5;
            channel[200] = 5;
        }
        let (mean, deviation) = Histogram { bins, pixels: 10 }.mean_and_std_dev();
        assert!((mean - 150.0).abs() < 1e-9);
        assert!((deviation - 50.0).abs() < 1e-9);
    }
}

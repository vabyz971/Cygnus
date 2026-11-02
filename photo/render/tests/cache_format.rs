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

//! The compute format on the adapter (docs/decisions/0026): all 65 536
//! 16-bit values, written as `v / 65535` by the packing function of the
//! shaders, then read back through a copy and through the unpacking
//! function, must come back exactly. Run on every GPU job of the CI
//! (lavapipe, WARP, Metal), so that no driver rounds differently.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use std::sync::mpsc;

use common::setup;
use cygnus_photo_render::format::{
    COMPUTE_CODEC_WGSL, COMPUTE_FORMAT, COMPUTE_FORMAT_WGSL, COMPUTE_SAMPLED_WGSL,
    COMPUTE_TEXEL_BYTES,
};

const SIDE: u32 = 256;

/// Writes texel i with channels (i, 65535 - i, i ^ 0x5555, 65535 - (i ^ 0x5555)),
/// then reads it back with `unpack_compute` and stores the values again as
/// integers in a buffer.
const SHADER: &str = "
@group(0) @binding(0) var target_texture: texture_storage_2d<COMPUTE_FORMAT, write>;
@group(0) @binding(1) var source_texture: texture_2d<COMPUTE_SAMPLED>;
@group(0) @binding(2) var<storage, read_write> unpacked: array<u32>;

fn expected(i: u32) -> vec4<u32> {
    let j = i ^ 0x5555u;
    return vec4<u32>(i, 65535u - i, j, 65535u - j);
}

@compute @workgroup_size(16, 16, 1)
fn write_values(@builtin(global_invocation_id) id: vec3<u32>) {
    let value = vec4<f32>(expected(id.y * 256u + id.x)) / 65535.0;
    textureStore(target_texture, vec2<i32>(id.xy), pack_compute(value));
}

@compute @workgroup_size(16, 16, 1)
fn read_values(@builtin(global_invocation_id) id: vec3<u32>) {
    let color = unpack_compute(textureLoad(source_texture, vec2<i32>(id.xy), 0));
    let v = vec4<u32>(round(color * 65535.0));
    let i = id.y * 256u + id.x;
    unpacked[2u * i] = v.x | (v.y << 16u);
    unpacked[2u * i + 1u] = v.z | (v.w << 16u);
}
";

fn expected(i: u32) -> [u16; 4] {
    let j = i ^ 0x5555;
    [i, 65535 - i, j, 65535 - j].map(|v| v as u16)
}

fn read_buffer(gpu: &cygnus_photo_render::gpu::Gpu, buffer: &wgpu::Buffer) -> Vec<u16> {
    let (sender, receiver) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap()
        });
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    receiver.recv().unwrap().unwrap();
    let mapped = buffer.slice(..).get_mapped_range().unwrap();
    bytemuck::cast_slice::<u8, u16>(&mapped).to_vec()
}

fn check(name: &str, values: &[u16]) -> usize {
    let wrong: Vec<(u32, [u16; 4], [u16; 4])> = values
        .as_chunks::<4>()
        .0
        .iter()
        .zip(0..)
        .filter(|(texel, i)| **texel != expected(*i))
        .map(|(texel, i)| (i, expected(i), *texel))
        .collect();
    for (i, expected, actual) in wrong.iter().take(8) {
        println!("{name}: texel {i}: expected {expected:?}, got {actual:?}");
    }
    wrong.len()
}

#[test]
fn every_16_bit_value_survives_the_compute_format() {
    let Some((gpu, _)) = setup() else { return };
    let source = format!("{COMPUTE_CODEC_WGSL}{SHADER}")
        .replace("COMPUTE_FORMAT", COMPUTE_FORMAT_WGSL)
        .replace("COMPUTE_SAMPLED", COMPUTE_SAMPLED_WGSL);
    let device = &gpu.device;
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: None,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let pipeline = |entry: &str| {
        device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: None,
            layout: None,
            module: &module,
            entry_point: Some(entry),
            compilation_options: Default::default(),
            cache: None,
        })
    };
    let (write, read) = (pipeline("write_values"), pipeline("read_values"));
    let texture = |usage| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d {
                width: SIDE,
                height: SIDE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: COMPUTE_FORMAT,
            usage,
            view_formats: &[],
        })
    };
    let written = texture(
        wgpu::TextureUsages::STORAGE_BINDING
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
    );
    // Separate texture for reading: one texture cannot be bound both as
    // storage and sampled in one dispatch.
    let sampled = texture(wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    let bytes = u64::from(SIDE * SIDE * COMPUTE_TEXEL_BYTES);
    let buffer = |usage| {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage,
            mapped_at_creation: false,
        })
    };
    let copied = buffer(wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
    let unpacked = buffer(wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC);
    let unpacked_read = buffer(wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST);
    let written_view = written.create_view(&Default::default());
    let sampled_view = sampled.create_view(&Default::default());
    let write_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &write.get_bind_group_layout(0),
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: wgpu::BindingResource::TextureView(&written_view),
        }],
    });
    let read_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &read.get_bind_group_layout(0),
        entries: &[
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&sampled_view),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: unpacked.as_entire_binding(),
            },
        ],
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&write);
        pass.set_bind_group(0, &write_group, &[]);
        pass.dispatch_workgroups(SIDE / 16, SIDE / 16, 1);
    }
    encoder.copy_texture_to_buffer(
        written.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &copied,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(SIDE * COMPUTE_TEXEL_BYTES),
                rows_per_image: Some(SIDE),
            },
        },
        written.size(),
    );
    encoder.copy_texture_to_texture(
        written.as_image_copy(),
        sampled.as_image_copy(),
        written.size(),
    );
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&read);
        pass.set_bind_group(0, &read_group, &[]);
        pass.dispatch_workgroups(SIDE / 16, SIDE / 16, 1);
    }
    encoder.copy_buffer_to_buffer(&unpacked, 0, &unpacked_read, 0, bytes);
    gpu.queue.submit([encoder.finish()]);

    let stored = check("stored", &read_buffer(&gpu, &copied));
    let read_back = check("unpacked", &read_buffer(&gpu, &unpacked_read));
    let line = format!(
        "compute format {COMPUTE_FORMAT:?} on {}: {} texels, stored wrong {stored}, unpacked wrong {read_back}",
        gpu.info.name,
        SIDE * SIDE
    );
    println!("{line}");
    if std::env::var("GITHUB_ACTIONS").is_ok() {
        println!("::notice title=compute format::{line}");
    }
    assert_eq!((stored, read_back), (0, 0), "{line}");
}

// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: Apache-2.0
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use std::marker::PhantomData;

use bytemuck::{Pod, Zeroable};
use wgpu::{util::DeviceExt, *};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    pub mvp: glam::Mat4,
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct SolidVertex {
    pub position: glam::Vec2,
    pub color: u32,
}

impl SolidVertex {
    pub const LAYOUT: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                offset: 0,
                format: VertexFormat::Float32x2,
                shader_location: 0,
            },
            VertexAttribute {
                offset: std::mem::size_of::<[f32; 2]>() as BufferAddress,
                format: VertexFormat::Unorm8x4,
                shader_location: 1,
            },
        ],
    };
}

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct GlyphVertex {
    pub position: glam::Vec2,
    pub tex_coords: glam::Vec2,
    pub color: u32,
}

impl GlyphVertex {
    pub const LAYOUT: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                offset: 0,
                format: VertexFormat::Float32x2,
                shader_location: 0,
            },
            VertexAttribute {
                offset: std::mem::size_of::<[f32; 2]>() as BufferAddress,
                format: VertexFormat::Float32x2,
                shader_location: 1,
            },
            VertexAttribute {
                offset: std::mem::size_of::<[f32; 4]>() as BufferAddress,
                format: VertexFormat::Unorm8x4,
                shader_location: 2,
            },
        ],
    };
}

pub struct DynamicMesh<T> {
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    index_num: u32,
    _data: PhantomData<T>,
}

impl<T: Pod> DynamicMesh<T> {
    pub fn new(device: &Device) -> Self {
        Self {
            vertex_buffer: device.create_buffer(&BufferDescriptor {
                label: Some("AlacrittyRoutine vertex buffer"),
                size: 0,
                mapped_at_creation: false,
                usage: BufferUsages::VERTEX,
            }),
            index_buffer: device.create_buffer(&BufferDescriptor {
                label: Some("AlacrittyRoutine vertex buffer"),
                size: 0,
                mapped_at_creation: false,
                usage: BufferUsages::INDEX,
            }),
            index_num: 0,
            _data: PhantomData,
        }
    }

    pub fn update(&mut self, device: &Device, vertices: &[T], indices: &[u32]) {
        self.vertex_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("AlacrittyRoutine vertex buffer"),
            contents: bytemuck::cast_slice(vertices),
            usage: BufferUsages::VERTEX,
        });

        self.index_buffer = device.create_buffer_init(&util::BufferInitDescriptor {
            label: Some("AlacrittyRoutine index buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: BufferUsages::INDEX,
        });

        self.index_num = indices.len() as u32;
    }

    pub fn draw<'a>(&'a self, rpass: &mut RenderPass<'a>) {
        rpass.set_vertex_buffer(0, self.vertex_buffer.slice(..));
        rpass.set_index_buffer(self.index_buffer.slice(..), IndexFormat::Uint32);
        rpass.draw_indexed(0..self.index_num, 0, 0..1);
    }
}

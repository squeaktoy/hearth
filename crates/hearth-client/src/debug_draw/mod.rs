// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// This file is part of Hearth.
//
// Hearth is free software: you can redistribute it and/or modify it under the
// terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Hearth is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Hearth. If not, see <https://www.gnu.org/licenses/>.

use std::{collections::HashMap, sync::Arc};

use bytemuck::{Pod, Zeroable};
use flume::{unbounded, Receiver, Sender};
use glam::{vec3, Vec3};
use hearth_core::{
    async_trait,
    process::factory::ProcessInfo,
    runtime::{Plugin, RuntimeBuilder},
    utils::*,
};
use hearth_rend3::{
    rend3::graph::{DepthHandle, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets},
    wgpu::*,
    Node, Rend3Plugin, Routine, RoutineInfo,
};
use hearth_terminal::rend3_alacritty::gpu::DynamicMesh;
use hearth_types::{debug_draw::*, Flags};

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CameraUniform {
    pub mvp: glam::Mat4,
}

/// GPU-ready debug draw vertex data.
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Vertex {
    pub position: Vec3,
    pub color: u32,
}

impl Vertex {
    pub const LAYOUT: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<Self>() as BufferAddress,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute {
                offset: 0,
                format: VertexFormat::Float32x4,
                shader_location: 0,
            },
            VertexAttribute {
                offset: std::mem::size_of::<[f32; 3]>() as BufferAddress,
                format: VertexFormat::Unorm8x4,
                shader_location: 1,
            },
        ],
    };
}

struct DebugDraw {
    mesh: DynamicMesh<Vertex>,
    hide: bool,
}

pub struct DebugDrawRoutine {
    device: Arc<Device>,
    queue: Arc<Queue>,
    camera_bind_group: BindGroup,
    camera_buffer: Buffer,
    pipeline: RenderPipeline,
    draws: HashMap<usize, DebugDraw>,
    update_rx: Receiver<(usize, DebugDrawUpdate)>,
}

impl Routine for DebugDrawRoutine {
    fn build_node(&mut self) -> Box<dyn Node + '_> {
        let mut updates = HashMap::new();

        while let Ok((id, update)) = self.update_rx.try_recv() {
            updates.insert(id, update);
        }

        for (id, update) in updates {
            let new_draw = || DebugDraw {
                mesh: DynamicMesh::new(self.device.as_ref(), Some(format!("debug draw #{id}"))),
                hide: false,
            };

            use DebugDrawUpdate::*;
            match update {
                Contents(mesh) => {
                    let draw = self.draws.entry(id).or_insert_with(new_draw);

                    let vertices: Vec<_> = mesh
                        .vertices
                        .into_iter()
                        .map(|v| Vertex {
                            position: v.position,
                            color: ((v.color[0] as u32) << 16)
                                | ((v.color[1] as u32) << 8)
                                | (v.color[2] as u32),
                        })
                        .collect();

                    draw.mesh.update(
                        self.device.as_ref(),
                        self.queue.as_ref(),
                        &vertices,
                        &mesh.indices,
                    );
                }
                Hide(hide) => {
                    let draw = self.draws.entry(id).or_insert_with(new_draw);
                    draw.hide = hide;
                }
                Destroy => {
                    self.draws.remove(&id);
                }
            }
        }

        Box::new(DebugDrawNode { routine: self })
    }
}

impl DebugDrawRoutine {
    pub fn new(rend3: &Rend3Plugin, update_rx: Receiver<(usize, DebugDrawUpdate)>) -> Self {
        let shader = rend3
            .iad
            .device
            .create_shader_module(&include_wgsl!("shaders.wgsl"));

        let bgl = rend3
            .iad
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("debug draw bind group layout"),
                entries: &[BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let layout = rend3
            .iad
            .device
            .create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("debug draw pipeline layout"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });

        let pipeline = rend3
            .iad
            .device
            .create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("debug draw pipeline"),
                layout: Some(&layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[Vertex::LAYOUT],
                },
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::LineList,
                    ..Default::default()
                },
                depth_stencil: Some(DepthStencilState {
                    format: TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: CompareFunction::GreaterEqual,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: MultisampleState::default(),
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[ColorTargetState {
                        format: rend3.surface_format,
                        blend: None,
                        write_mask: ColorWrites::COLOR,
                    }],
                }),
                multiview: None,
            });

        let camera_buffer = rend3.iad.device.create_buffer(&BufferDescriptor {
            label: Some("debug draw camera buffer"),
            size: std::mem::size_of::<CameraUniform>() as BufferAddress,
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let camera_bind_group = rend3.iad.device.create_bind_group(&BindGroupDescriptor {
            label: Some("debug draw bind group"),
            layout: &bgl,
            entries: &[BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        Self {
            device: rend3.iad.device.to_owned(),
            queue: rend3.iad.queue.to_owned(),
            camera_buffer,
            camera_bind_group,
            pipeline,
            draws: HashMap::new(),
            update_rx,
        }
    }
}

struct DebugDrawNode<'a> {
    routine: &'a DebugDrawRoutine,
}

impl<'a> Node<'a> for DebugDrawNode<'a> {
    fn draw<'graph>(&'graph self, info: &mut RoutineInfo<'_, 'graph>) {
        let output = info.graph.add_surface_texture();
        let depth = info.state.depth;

        let mut builder = info.graph.add_node("debug draw");
        let output_handle = builder.add_render_target_output(output);
        let depth_handle = builder.add_render_target_input(depth);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: Some(RenderPassDepthTarget {
                target: DepthHandle::RenderTarget(depth_handle),
                depth_clear: None,
                stencil_clear: None,
            }),
        });

        let routine = builder.passthrough_ref(self.routine);

        builder.build(
            move |pt, _renderer, encoder_or_pass, _temps, _ready, graph_data| {
                let routine = pt.get(routine);
                let rpass = encoder_or_pass.get_rpass(rpass_handle);
                let mvp = graph_data.camera_manager.view_proj();

                routine.queue.write_buffer(
                    &routine.camera_buffer,
                    0,
                    bytemuck::bytes_of(&CameraUniform { mvp }),
                );

                rpass.set_pipeline(&routine.pipeline);
                rpass.set_bind_group(0, &routine.camera_bind_group, &[]);

                for draw in routine.draws.values() {
                    if draw.hide {
                        continue;
                    }

                    draw.mesh.draw(rpass);
                }
            },
        );
    }
}

pub struct DebugDrawInstance {
    id: usize,
    destroyed: bool,
    update_tx: Sender<(usize, DebugDrawUpdate)>,
}

impl Drop for DebugDrawInstance {
    fn drop(&mut self) {
        if !self.destroyed {
            let _ = self.update_tx.send((self.id, DebugDrawUpdate::Destroy));
        }
    }
}

#[async_trait]
impl SinkProcess for DebugDrawInstance {
    type Message = DebugDrawUpdate;

    async fn on_message(&mut self, request: &mut RequestInfo<'_, Self::Message>) {
        if self.destroyed {
            return;
        }

        if let DebugDrawUpdate::Destroy = request.data {
            self.destroyed = true;
        }

        let _ = self.update_tx.send((self.id, request.data.clone()));
    }
}

pub struct DebugDrawFactory {
    next_id: usize,
    update_tx: Sender<(usize, DebugDrawUpdate)>,
}

#[async_trait]
impl RequestResponseProcess for DebugDrawFactory {
    type Request = ();
    type Response = ();

    async fn on_request(
        &mut self,
        request: &mut RequestInfo<'_, Self::Request>,
    ) -> ResponseInfo<Self::Response> {
        let instance = DebugDrawInstance {
            id: self.next_id,
            destroyed: false,
            update_tx: self.update_tx.clone(),
        };

        self.next_id += 1;

        let info = ProcessInfo {};
        let flags = Flags::SEND | Flags::KILL;
        let child = request.runtime.process_factory.spawn(info, flags);
        let child_cap = request.ctx.copy_self_capability(&child);

        tokio::spawn(instance.run(
            "DebugDrawInstance".to_string(),
            request.runtime.clone(),
            child,
        ));

        ResponseInfo {
            data: (),
            caps: vec![child_cap],
        }
    }
}

impl ServiceRunner for DebugDrawFactory {
    const NAME: &'static str = "hearth.DebugDrawFactory";
}

#[derive(Default)]
pub struct DebugDrawPlugin {}

impl Plugin for DebugDrawPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        let rend3 = builder
            .get_plugin_mut::<Rend3Plugin>()
            .expect("rend3 plugin was not found");

        let (update_tx, update_rx) = unbounded();

        let mut vertices = Vec::new();

        let size = 100;
        let scale = 0.1;
        let color = [0, 255, 0];

        for x in -size..size {
            vertices.push(DebugDrawVertex {
                position: vec3(x as f32, 0.0, -size as f32) * scale,
                color,
            });

            vertices.push(DebugDrawVertex {
                position: vec3(x as f32, 0.0, size as f32) * scale,
                color,
            });
        }

        for y in -size..size {
            vertices.push(DebugDrawVertex {
                position: vec3(-size as f32, 0.0, y as f32) * scale,
                color,
            });

            vertices.push(DebugDrawVertex {
                position: vec3(size as f32, 0.0, y as f32) * scale,
                color,
            });
        }

        let _ = update_tx.send((
            0,
            DebugDrawUpdate::Contents(DebugDrawMesh {
                indices: (0..(vertices.len() as u32)).collect(),
                vertices,
            }),
        ));

        rend3.add_routine(DebugDrawRoutine::new(rend3, update_rx));

        builder.add_plugin(DebugDrawFactory {
            next_id: 1,
            update_tx,
        });
    }
}

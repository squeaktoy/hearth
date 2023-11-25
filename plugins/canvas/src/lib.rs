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
use flume::{Receiver, Sender};
use hearth_rend3::{
    rend3::{
        graph::{DepthHandle, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets},
        types::glam::{vec2, Mat4},
    },
    wgpu::{util::DeviceExt, *},
    Node, Rend3Plugin, Routine, RoutineInfo,
};
use hearth_runtime::{
    async_trait, cargo_process_metadata,
    flue::Permissions,
    hearth_schema::canvas::*,
    process::ProcessMetadata,
    runtime::{Plugin, RuntimeBuilder},
    tokio,
    utils::*,
};

/// A specific kind of operation on a canvas.
pub enum CanvasOperationKind {
    /// Create a new canvas with this ID.
    Create { position: Position, pixels: Pixels },

    /// Destroy this canvas.
    Destroy,

    /// Update this canvas.
    Update(CanvasUpdate),
}

/// A message sent from a canvas instance to the canvas routine.
///
/// Contains an identifier for the canvas and an operation kind on it.
pub type CanvasOperation = (usize, CanvasOperationKind);

/// GPU-side canvas rendering uniform data.
#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct CanvasUniform {
    pub mvp: Mat4,
}

/// A canvas's GPU state.
pub struct CanvasDraw {
    position: Position,
    ubo: Buffer,
    width: u32,
    height: u32,
    texture: Texture,
    bind_group: BindGroup,
}

impl CanvasDraw {
    pub fn new(
        device: &Device,
        queue: &Queue,
        bgl: &BindGroupLayout,
        sampler: &Sampler,
        position: Position,
        pixels: Pixels,
    ) -> Self {
        let ubo = device.create_buffer(&BufferDescriptor {
            label: Some("canvas uniform"),
            size: std::mem::size_of::<CanvasUniform>() as BufferAddress,
            usage: BufferUsages::COPY_DST | BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });

        let width = pixels.width;
        let height = pixels.height;
        let texture = Self::create_texture(device, queue, pixels);
        let bind_group = Self::create_bind_group(device, bgl, &ubo, &texture, sampler);

        Self {
            position,
            ubo,
            texture,
            width,
            height,
            bind_group,
        }
    }

    pub fn resize(
        &mut self,
        device: &Device,
        queue: &Queue,
        pixels: Pixels,
        bgl: &BindGroupLayout,
        sampler: &Sampler,
    ) {
        // don't allocate a new texture if the size is the same. just blit.
        if self.width == pixels.width && self.height == pixels.height {
            let blit = Blit { x: 0, y: 0, pixels };
            self.blit(queue, blit);
            return;
        }

        self.width = pixels.width;
        self.height = pixels.height;
        self.texture = Self::create_texture(device, queue, pixels);
        self.bind_group = Self::create_bind_group(device, bgl, &self.ubo, &self.texture, sampler);
    }

    /// Update this buffer's position.
    ///
    /// Does nothing until [Self::update_ubo] is called.
    pub fn set_position(&mut self, position: Position) {
        self.position = position;
    }

    /// Updates this draw's uniform buffer on the GPU.
    pub fn update_ubo(&self, queue: &Queue, vp: Mat4) {
        let half_size = vec2(self.position.half_size.x, -self.position.half_size.y);
        let scale = Mat4::from_scale(half_size.extend(1.0));
        let rotation = Mat4::from_quat(self.position.orientation);
        let translation = Mat4::from_translation(self.position.origin);
        let mvp = vp * translation * rotation * scale;

        let ubo = CanvasUniform { mvp };

        queue.write_buffer(&self.ubo, 0, bytemuck::bytes_of(&ubo));
    }

    pub fn blit(&self, queue: &Queue, mut blit: Blit) {
        // available width and height
        let aw = self.width.saturating_sub(blit.x);
        let ah = self.height.saturating_sub(blit.y);

        // consumed width and height
        let width = blit.pixels.width.max(aw);
        let height = blit.pixels.height.max(ah);

        // abort if the copy has no area
        if width == 0 || height == 0 {
            return;
        }

        // correct the pixel data length
        blit.pixels
            .data
            .resize((blit.pixels.width * blit.pixels.height) as usize * 4, 0xff);

        queue.write_texture(
            ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: Origin3d {
                    x: blit.x,
                    y: blit.y,
                    z: 0,
                },
                aspect: TextureAspect::All,
            },
            &blit.pixels.data,
            ImageDataLayout {
                offset: 0,
                bytes_per_row: Some((blit.pixels.width * 4).try_into().unwrap()),
                rows_per_image: Some((blit.pixels.height).try_into().unwrap()),
            },
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
    }

    fn create_texture(device: &Device, queue: &Queue, mut pixels: Pixels) -> Texture {
        // correct the pixel data length
        pixels
            .data
            .resize((pixels.width * pixels.height) as usize * 4, 0xff);

        device.create_texture_with_data(
            queue,
            &TextureDescriptor {
                label: Some("canvas texture"),
                size: Extent3d {
                    width: pixels.width,
                    height: pixels.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: TextureDimension::D2,
                format: TextureFormat::Rgba8UnormSrgb,
                usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            },
            &pixels.data,
        )
    }

    fn create_bind_group(
        device: &Device,
        bgl: &BindGroupLayout,
        ubo: &Buffer,
        texture: &Texture,
        sampler: &Sampler,
    ) -> BindGroup {
        device.create_bind_group(&BindGroupDescriptor {
            label: Some("canvas bind group"),
            layout: bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::Buffer(ubo.as_entire_buffer_binding()),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::TextureView(
                        &texture.create_view(&Default::default()),
                    ),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: BindingResource::Sampler(sampler),
                },
            ],
        })
    }
}

/// The canvas rend3 draw routine.
pub struct CanvasRoutine {
    ops_rx: Receiver<CanvasOperation>,
    device: Arc<Device>,
    queue: Arc<Queue>,
    bgl: BindGroupLayout,
    pipeline: RenderPipeline,
    sampler: Sampler,
    draws: HashMap<usize, CanvasDraw>,
}

impl CanvasRoutine {
    fn new(rend3: &mut Rend3Plugin, ops_rx: Receiver<CanvasOperation>) -> Self {
        let device = rend3.iad.device.as_ref();

        let shader = device.create_shader_module(&include_wgsl!("shaders.wgsl"));

        let bgl = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("canvas bind group layout"),
            entries: &[
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::VERTEX_FRAGMENT,
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: TextureSampleType::Float { filterable: true },
                        view_dimension: TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                BindGroupLayoutEntry {
                    binding: 2,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: Some("canvas pipeline layout"),
            bind_group_layouts: &[&bgl],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&RenderPipelineDescriptor {
            label: Some("canvas pipeline"),
            layout: Some(&layout),
            vertex: VertexState {
                module: &shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: PrimitiveState {
                topology: PrimitiveTopology::TriangleStrip,
                ..Default::default()
            },
            depth_stencil: Some(DepthStencilState {
                format: TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: CompareFunction::GreaterEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
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

        let sampler = device.create_sampler(&SamplerDescriptor {
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Nearest,
            min_filter: FilterMode::Nearest,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        Self {
            ops_rx,
            device: rend3.iad.device.to_owned(),
            queue: rend3.iad.queue.to_owned(),
            bgl,
            pipeline,
            sampler,
            draws: HashMap::new(),
        }
    }
}

impl Routine for CanvasRoutine {
    fn build_node(&mut self) -> Box<dyn Node + '_> {
        for (id, operation) in self.ops_rx.drain() {
            let update = match operation {
                CanvasOperationKind::Update(update) => update,
                CanvasOperationKind::Create { position, pixels } => {
                    self.draws.insert(
                        id,
                        CanvasDraw::new(
                            &self.device,
                            &self.queue,
                            &self.bgl,
                            &self.sampler,
                            position,
                            pixels,
                        ),
                    );

                    continue;
                }
                CanvasOperationKind::Destroy => {
                    self.draws.remove(&id);
                    continue;
                }
            };

            let Some(draw) = self.draws.get_mut(&id) else {
                continue;
            };

            match update {
                CanvasUpdate::Relocate(position) => draw.set_position(position),
                CanvasUpdate::Blit(blit) => draw.blit(&self.queue, blit),
                CanvasUpdate::Resize(pixels) => {
                    draw.resize(&self.device, &self.queue, pixels, &self.bgl, &self.sampler)
                }
            }
        }

        Box::new(CanvasNode { routine: self })
    }
}

/// The canvas rend3 render node.
pub struct CanvasNode<'a> {
    routine: &'a CanvasRoutine,
}

impl<'a> Node<'a> for CanvasNode<'a> {
    fn draw<'graph>(&'graph self, info: &mut RoutineInfo<'_, 'graph>) {
        let output = info.graph.add_surface_texture();
        let depth = info.state.depth;

        let mut builder = info.graph.add_node("canvas");
        let output_handle = builder.add_render_target_output(output);
        let depth_handle = builder.add_render_target_output(depth);

        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: Some(RenderPassDepthTarget {
                target: DepthHandle::RenderTarget(depth_handle),
                depth_clear: Some(0.0),
                stencil_clear: None,
            }),
        });

        let routine = builder.passthrough_ref(self.routine);

        builder.build(
            move |pt, _renderer, encoder_or_pass, _temps, _ready, graph_data| {
                let routine = pt.get(routine);
                let rpass = encoder_or_pass.get_rpass(rpass_handle);
                let vp = graph_data.camera_manager.view_proj();

                rpass.set_pipeline(&routine.pipeline);

                for draw in routine.draws.values() {
                    draw.update_ubo(&routine.queue, vp);
                    rpass.set_bind_group(0, &draw.bind_group, &[]);
                    rpass.draw(0..4, 0..1);
                }
            },
        );
    }
}

/// A canvas process. Processes [CanvasUpdate].
pub struct CanvasInstance {
    /// This canvas's ID.
    id: usize,

    /// A sender to the canvas routine.
    ops_tx: Sender<CanvasOperation>,
}

impl Drop for CanvasInstance {
    fn drop(&mut self) {
        let _ = self.ops_tx.send((self.id, CanvasOperationKind::Destroy));
    }
}

#[async_trait]
impl SinkProcess for CanvasInstance {
    type Message = CanvasUpdate;

    async fn on_message<'a>(&'a mut self, message: MessageInfo<'a, Self::Message>) {
        let _ = self
            .ops_tx
            .send((self.id, CanvasOperationKind::Update(message.data)));
    }
}

/// Spawns [CanvasInstance] and processes [FactoryRequest].
pub struct CanvasFactory {
    /// The ID of the next canvas that will be spawned.
    next_id: usize,

    /// A sender to the canvas routine.
    ops_tx: Sender<CanvasOperation>,
}

#[async_trait]
impl RequestResponseProcess for CanvasFactory {
    type Request = FactoryRequest;
    type Response = FactoryResponse;

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response> {
        match &request.data {
            FactoryRequest::CreateCanvas { position, pixels } => {
                // allocate a new ID
                let id = self.next_id;
                self.next_id += 1;

                // update the routine with the initial position
                let _ = self.ops_tx.send((
                    id,
                    CanvasOperationKind::Create {
                        position: position.to_owned(),
                        pixels: pixels.to_owned(),
                    },
                ));

                // create the instance
                let instance = CanvasInstance {
                    id,
                    ops_tx: self.ops_tx.clone(),
                };

                // initialize the instance's metadata
                let mut meta = cargo_process_metadata!();
                meta.name = Some("CanvasInstance".to_string());
                meta.description = Some("An instance of a canvas".to_string());

                // spawn the instance child process
                let child = request.runtime.process_factory.spawn(meta);

                // retrieve the child's parent cap
                let perms = Permissions::SEND | Permissions::KILL;
                let child_cap = child
                    .borrow_parent()
                    .export_to(perms, request.process.borrow_table())
                    .unwrap();

                // execute the instance process runner
                let runtime = request.runtime.clone();
                tokio::spawn(async move {
                    instance
                        .run("CanvasInstance".to_string(), runtime, &child)
                        .await;
                });

                // respond with the new canvas
                ResponseInfo {
                    data: Ok(FactorySuccess::Canvas),
                    caps: vec![child_cap],
                }
            }
        }
    }
}

impl ServiceRunner for CanvasFactory {
    const NAME: &'static str = "hearth.canvas.CanvasFactory";

    fn get_process_metadata() -> hearth_runtime::process::ProcessMetadata {
        let mut meta = cargo_process_metadata!();

        meta.description =
            Some("The native canvas factory service. Accepts FactoryRequest".to_string());

        meta
    }
}

/// A plugin that provides Hearth "canvases", rectangular 3D textures with
/// efficient updating support.
#[derive(Debug, Default)]
pub struct CanvasPlugin;

impl Plugin for CanvasPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        let rend3 = builder
            .get_plugin_mut::<Rend3Plugin>()
            .expect("rend3 plugin was not found");

        let (ops_tx, ops_rx) = flume::unbounded();
        let routine = CanvasRoutine::new(rend3, ops_rx);
        rend3.add_routine(routine);
        builder.add_plugin(CanvasFactory { next_id: 0, ops_tx });
    }
}

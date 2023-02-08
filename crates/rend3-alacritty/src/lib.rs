use bytemuck::{Pod, Zeroable};
use font_mud::glyph_atlas::GlyphAtlas;
use rend3::graph::{
    RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets, RenderTargetHandle,
};
use rend3::Renderer;
use wgpu::util::DeviceExt;
use wgpu::*;

#[repr(C)]
#[derive(Copy, Clone, Debug, Pod, Zeroable)]
pub struct Vertex {
    pub position: glam::Vec2,
    pub tex_coords: glam::Vec2,
}

impl Vertex {
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
        ],
    };
}

pub struct AlacrittyRoutine {
    glyph_atlas: GlyphAtlas,
    atlas_texture: Texture,
    atlas_view: TextureView,
    atlas_sampler: Sampler,
    bind_group: BindGroup,
    pipeline: RenderPipeline,
    vertex_buffer: Buffer,
    index_buffer: Buffer,
    index_num: u32,
}

impl AlacrittyRoutine {
    /// This routine runs after tonemapping, so `format` is the format of the
    /// final swapchain image format.
    pub fn new(face: &ttf_parser::Face, renderer: &Renderer, format: TextureFormat) -> Self {
        let (glyph_atlas, _errors) = font_mud::glyph_atlas::GlyphAtlas::new(&face).unwrap();

        let atlas_size = Extent3d {
            width: glyph_atlas.bitmap.width as u32,
            height: glyph_atlas.bitmap.height as u32,
            depth_or_array_layers: 1,
        };

        let atlas_texture = renderer.device.create_texture(&TextureDescriptor {
            label: Some("AlacrittyRoutine::glyph_texture"),
            size: atlas_size.clone(),
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        });

        renderer.queue.write_texture(
            ImageCopyTexture {
                texture: &atlas_texture,
                mip_level: 0,
                origin: Origin3d::ZERO,
                aspect: TextureAspect::All,
            },
            glyph_atlas.bitmap.data_bytes(),
            ImageDataLayout {
                offset: 0,
                bytes_per_row: std::num::NonZeroU32::new(4 * glyph_atlas.bitmap.width as u32),
                rows_per_image: std::num::NonZeroU32::new(glyph_atlas.bitmap.height as u32),
            },
            atlas_size,
        );

        let atlas_view = atlas_texture.create_view(&Default::default());
        let atlas_sampler = renderer.device.create_sampler(&SamplerDescriptor {
            address_mode_u: AddressMode::ClampToEdge,
            address_mode_v: AddressMode::ClampToEdge,
            address_mode_w: AddressMode::ClampToEdge,
            mag_filter: FilterMode::Linear,
            min_filter: FilterMode::Linear,
            mipmap_filter: FilterMode::Nearest,
            ..Default::default()
        });

        let shader_desc = include_wgsl!("shader.wgsl");
        let shader = renderer.device.create_shader_module(&shader_desc);

        let bgl = renderer
            .device
            .create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("AlacrittyRoutine bind group layout"),
                entries: &[
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Texture {
                            multisampled: false,
                            view_dimension: TextureViewDimension::D2,
                            sample_type: TextureSampleType::Float { filterable: true },
                        },
                        count: None,
                    },
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::FRAGMENT,
                        ty: BindingType::Sampler(SamplerBindingType::Filtering),
                        count: None,
                    },
                ],
            });

        let bind_group = renderer.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &bgl,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: BindingResource::TextureView(&atlas_view),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: BindingResource::Sampler(&atlas_sampler),
                },
            ],
        });

        let layout = renderer
            .device
            .create_pipeline_layout(&PipelineLayoutDescriptor {
                label: Some("AlacrittyRoutine pipeline layout"),
                bind_group_layouts: &[&bgl],
                push_constant_ranges: &[],
            });

        let pipeline = renderer
            .device
            .create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("AlacrittyRoutine::pipeline"),
                layout: Some(&layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[Vertex::LAYOUT],
                },
                depth_stencil: Some(DepthStencilState {
                    format: TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: CompareFunction::GreaterEqual,
                    stencil: StencilState::default(),
                    bias: DepthBiasState::default(),
                }),
                primitive: PrimitiveState {
                    topology: PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: FrontFace::Ccw,
                    cull_mode: Some(Face::Back),
                    unclipped_depth: false,
                    polygon_mode: PolygonMode::Fill,
                    conservative: false,
                },
                multisample: MultisampleState::default(),
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[ColorTargetState {
                        format,
                        blend: Some(BlendState::ALPHA_BLENDING),
                        write_mask: ColorWrites::COLOR,
                    }],
                }),
                multiview: None,
            });

        let text = "According to all known laws of aviation...";
        let mut vertices = Vec::new();
        let mut indices = Vec::new();

        let mut offset = glam::Vec2::new(-0.8, 0.0);
        let atlas_bitmap = &glyph_atlas.bitmap;
        let atlas_size = glam::Vec2::new(atlas_bitmap.width as f32, atlas_bitmap.height as f32);
        for c in text.chars() {
            offset.x += 0.03;

            if let Some(glyph) = face.glyph_index(c) {
                let index = vertices.len() as u32;
                let bitmap = match glyph_atlas.glyphs[glyph.0 as usize].as_ref() {
                    Some(b) => b,
                    None => continue,
                };

                let anchor = glam::Vec2::from(bitmap.anchor);

                let position = bitmap.position;
                let position = glam::Vec2::new(position.0 as f32, position.1 as f32);
                let position = position / atlas_size;

                let size = bitmap.size;
                let size = glam::Vec2::new(size.0 as f32, size.1 as f32);
                let size = size / atlas_size;

                let v1 = glam::Vec2::ZERO;
                let v2 = glam::Vec2::new(size.x, 0.0);
                let v3 = glam::Vec2::new(0.0, size.y);
                let v4 = size;

                vertices.extend([v1, v2, v3, v4].iter().map(|v| Vertex {
                    position: offset + *v - anchor,
                    tex_coords: position + *v,
                }));

                indices.extend_from_slice(&[
                    index,
                    index + 1,
                    index + 2,
                    index + 2,
                    index + 1,
                    index + 3,
                ]);
            }
        }

        let vertex_buffer = renderer
            .device
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("AlacrittyRoutine vertex buffer"),
                contents: bytemuck::cast_slice(vertices.as_slice()),
                usage: BufferUsages::VERTEX,
            });

        let index_buffer = renderer
            .device
            .create_buffer_init(&util::BufferInitDescriptor {
                label: Some("AlacrittyRoutine index buffer"),
                contents: bytemuck::cast_slice(indices.as_slice()),
                usage: BufferUsages::INDEX,
            });

        let index_num = indices.len() as u32;

        Self {
            glyph_atlas,
            atlas_texture,
            atlas_view,
            atlas_sampler,
            bind_group,
            pipeline,
            vertex_buffer,
            index_buffer,
            index_num,
        }
    }

    pub fn add_to_graph<'node>(
        &'node mut self,
        graph: &mut RenderGraph<'node>,
        output: RenderTargetHandle,
        depth: RenderTargetHandle,
    ) {
        let mut builder = graph.add_node("alacritty");
        let output_handle = builder.add_render_target_output(output);
        let depth_handle = builder.add_render_target_input(depth);
        let rpass_handle = builder.add_renderpass(RenderPassTargets {
            targets: vec![RenderPassTarget {
                color: output_handle,
                clear: Color::BLACK,
                resolve: None,
            }],
            depth_stencil: Some(RenderPassDepthTarget {
                target: rend3::graph::DepthHandle::RenderTarget(depth_handle),
                depth_clear: None,
                stencil_clear: None,
            }),
        });

        let pt_handle = builder.passthrough_ref(self);

        builder.build(
            move |pt, renderer, encoder_or_pass, temps, ready, graph_data| {
                let this = pt.get(pt_handle);
                let rpass = encoder_or_pass.get_rpass(rpass_handle);
                rpass.set_pipeline(&this.pipeline);
                rpass.set_bind_group(0, &this.bind_group, &[]);
                rpass.set_vertex_buffer(0, this.vertex_buffer.slice(..));
                rpass.set_index_buffer(this.index_buffer.slice(..), IndexFormat::Uint32);
                rpass.draw_indexed(0..this.index_num, 0, 0..1);
            },
        );
    }
}

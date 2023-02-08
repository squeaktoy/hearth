use rend3::Renderer;
use wgpu::{RenderPipeline, TextureFormat};

pub struct AlacrittyRoutine {
    pipeline: RenderPipeline,
}

impl AlacrittyRoutine {
    /// This routine runs after tonemapping, so `format` is the format of the
    /// final swapchain image format.
    pub fn new(renderer: &Renderer, format: TextureFormat) -> Self {
        let shader_desc = wgpu::include_wgsl!("shader.wgsl");
        let shader = renderer.device.create_shader_module(&shader_desc);

        let layout = renderer
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("AlacrittyRoutine pipeline layout"),
                bind_group_layouts: &[],
                push_constant_ranges: &[],
            });

        let pipeline = renderer
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("AlacrittyRoutine::pipeline"),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[],
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: wgpu::TextureFormat::Depth32Float,
                    depth_write_enabled: false,
                    depth_compare: wgpu::CompareFunction::GreaterEqual,
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    unclipped_depth: false,
                    polygon_mode: wgpu::PolygonMode::Fill,
                    conservative: false,
                },
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::COLOR,
                    }],
                }),
                multiview: None,
            });

        Self { pipeline }
    }
}

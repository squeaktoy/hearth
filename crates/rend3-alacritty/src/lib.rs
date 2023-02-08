use rend3::graph::{
    RenderGraph, RenderPassDepthTarget, RenderPassTarget, RenderPassTargets, RenderTargetHandle,
};
use rend3::Renderer;
use wgpu::{Color, RenderPipeline, TextureFormat};

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
                rpass.draw(0..3, 0..1);
            },
        );
    }
}

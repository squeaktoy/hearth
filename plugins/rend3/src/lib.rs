use std::sync::Arc;

use glam::{UVec2, Vec4};
use hearth_runtime::runtime::{Plugin, RuntimeBuilder};
use rend3::graph::{ReadyData, RenderGraph};
use rend3::types::{Camera, SampleCount, TextureHandle};
use rend3::util::output::OutputFrame;
use rend3::{InstanceAdapterDevice, Renderer};
use rend3_routine::base::{BaseRenderGraph, BaseRenderGraphIntermediateState};
use rend3_routine::pbr::PbrRoutine;
use rend3_routine::skybox::SkyboxRoutine;
use rend3_routine::tonemapping::TonemappingRoutine;
use tokio::sync::{mpsc, oneshot};
use wgpu::TextureFormat;

pub use rend3;
pub use rend3_routine;
pub use wgpu;

pub mod utils;

/// The info about a frame passed to [Routine::draw].
pub struct RoutineInfo<'a, 'graph> {
    pub state: &'a BaseRenderGraphIntermediateState,
    pub sample_count: SampleCount,
    pub resolution: UVec2,
    pub ready_data: &'a ReadyData,
    pub graph: &'a mut RenderGraph<'graph>,
}

pub trait Routine: Send + Sync + 'static {
    fn build_node(&mut self) -> Box<dyn Node<'_> + '_>;
}

pub trait Node<'a> {
    fn draw<'graph>(&'graph self, info: &mut RoutineInfo<'_, 'graph>);
}

/// A request to the renderer to draw a single frame.
pub struct FrameRequest {
    /// The rend3-ready output frame.
    pub output_frame: OutputFrame,

    /// The dimensions of the frame.
    pub resolution: glam::UVec2,

    /// The camera to use for this frame.
    pub camera: Camera,

    /// This oneshot message is sent when the frame is done rendering.
    pub on_complete: oneshot::Sender<()>,
}

/// An update to the global rend3 state.
pub enum Rend3Command {
    /// Updates the skybox.
    SetSkybox(TextureHandle),

    /// Updates the ambient lighting.
    SetAmbient(Vec4),
}

/// A rend3 Hearth plugin for adding 3D rendering to a Hearth runtime.
///
/// This plugin can be acquired by other plugins during runtime building to add
/// more nodes to the render graph.
pub struct Rend3Plugin {
    pub iad: InstanceAdapterDevice,
    pub surface_format: TextureFormat,
    pub renderer: Arc<Renderer>,
    pub base_render_graph: BaseRenderGraph,
    pub pbr_routine: PbrRoutine,
    pub tonemapping_routine: TonemappingRoutine,
    pub skybox_routine: SkyboxRoutine,
    pub ambient: Vec4,
    pub frame_request_tx: mpsc::UnboundedSender<FrameRequest>,
    pub command_tx: mpsc::UnboundedSender<Rend3Command>,
    new_skybox: Option<TextureHandle>,
    frame_request_rx: mpsc::UnboundedReceiver<FrameRequest>,
    command_rx: mpsc::UnboundedReceiver<Rend3Command>,
    routines: Vec<Box<dyn Routine>>,
}

impl Plugin for Rend3Plugin {
    fn finalize(mut self, _builder: &mut RuntimeBuilder) {
        tokio::spawn(async move {
            while let Some(frame) = self.frame_request_rx.recv().await {
                self.flush_commands();
                self.draw(frame);
            }
        });
    }
}

impl Rend3Plugin {
    /// Creates a new rend3 plugin from an existing [InstanceAdapterDevice] and
    /// the target window's texture format.
    pub fn new(iad: InstanceAdapterDevice, surface_format: TextureFormat) -> Self {
        let handedness = rend3::types::Handedness::Right;
        let renderer = Renderer::new(iad.to_owned(), handedness, None).unwrap();
        let base_render_graph = BaseRenderGraph::new(&renderer);
        let mut data_core = renderer.data_core.lock();
        let interfaces = &base_render_graph.interfaces;
        let pbr_routine = PbrRoutine::new(&renderer, &mut data_core, interfaces);
        let tonemapping_routine = TonemappingRoutine::new(&renderer, interfaces, surface_format);
        let skybox_routine = SkyboxRoutine::new(&renderer, interfaces);
        drop(data_core);

        let (frame_request_tx, frame_request_rx) = mpsc::unbounded_channel();
        let (command_tx, command_rx) = mpsc::unbounded_channel();

        Self {
            iad,
            surface_format,
            renderer,
            base_render_graph,
            pbr_routine,
            tonemapping_routine,
            skybox_routine,
            frame_request_tx,
            frame_request_rx,
            command_tx,
            command_rx,
            new_skybox: None,
            ambient: Vec4::ZERO,
            routines: Vec::new(),
        }
    }

    /// Adds a new [Routine] to this plugin.
    pub fn add_routine(&mut self, routine: impl Routine) {
        self.routines.push(Box::new(routine));
    }

    /// Flushes and applies all [Rend3Command] messages.
    pub fn flush_commands(&mut self) {
        while let Ok(command) = self.command_rx.try_recv() {
            use Rend3Command::*;
            match command {
                SetSkybox(texture) => {
                    self.new_skybox = Some(texture);
                }
                SetAmbient(ambient) => {
                    self.ambient = ambient;
                }
            }
        }
    }

    /// Draws a frame in response to a [FrameRequest].
    pub fn draw(&mut self, request: FrameRequest) {
        let (cmd_bufs, ready) = self.renderer.ready();

        if let Some(skybox) = self.new_skybox.take() {
            self.skybox_routine.set_background_texture(Some(skybox));
            self.skybox_routine.ready(&self.renderer);
        }

        let aspect = request.resolution.as_vec2();
        let aspect = aspect.x / aspect.y;
        self.renderer.set_aspect_ratio(aspect);
        self.renderer.set_camera_data(request.camera);

        let nodes: Vec<_> = self
            .routines
            .iter_mut()
            .map(|routine| routine.build_node())
            .collect();

        let mut graph_data = RenderGraph::new();
        let graph = &mut graph_data;
        let samples = SampleCount::One;
        let base = &self.base_render_graph;
        let ambient = self.ambient;
        let pbr = &self.pbr_routine;
        let skybox = Some(&self.skybox_routine);

        // see implementation of BaseRenderGraph::add_to_graph() for details
        // on what the following code is based on
        //
        // we need to override this function so that we can hook into the
        // graph's state in our custom nodes
        let state =
            BaseRenderGraphIntermediateState::new(graph, &ready, request.resolution, samples);

        // Preparing and uploading data
        state.pre_skinning(graph);
        state.pbr_pre_culling(graph);
        state.create_frame_uniforms(graph, base, ambient);

        // Skinning
        state.skinning(graph, base);

        // Culling
        state.pbr_shadow_culling(graph, base, pbr);
        state.pbr_culling(graph, base, pbr);

        // Depth-only rendering
        state.pbr_shadow_rendering(graph, pbr);
        state.pbr_prepass_rendering(graph, pbr, samples);

        // Skybox
        state.skybox(graph, skybox, samples);

        // Forward rendering
        state.pbr_forward_rendering(graph, pbr, samples);

        // Make the reference to the surface
        let surface = graph.add_surface_texture();
        state.tonemapping(graph, &self.tonemapping_routine, surface);

        let mut info = RoutineInfo {
            state: &state,
            sample_count: SampleCount::One,
            resolution: request.resolution,
            ready_data: &ready,
            graph,
        };

        for node in nodes.iter() {
            node.draw(&mut info);
        }

        graph_data.execute(&self.renderer, request.output_frame, cmd_bufs, &ready);

        let _ = request.on_complete.send(()); // ignore hangup
    }
}

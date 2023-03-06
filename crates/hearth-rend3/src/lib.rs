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

use std::sync::Arc;

use hearth_core::async_trait;
use hearth_core::runtime::{Plugin, Runtime, RuntimeBuilder};
use rend3::graph::RenderGraph;
use rend3::types::{SampleCount, Camera};
use rend3::util::output::OutputFrame;
use rend3::{InstanceAdapterDevice, Renderer};
use rend3_routine::base::BaseRenderGraph;
use rend3_routine::pbr::PbrRoutine;
use rend3_routine::tonemapping::TonemappingRoutine;
use tokio::sync::{oneshot, mpsc};
use wgpu::TextureFormat;

pub use rend3;
pub use rend3_routine;
pub use wgpu;

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

/// A rend3 Hearth plugin for adding 3D rendering to a Hearth runtime.
///
/// This plugin can be acquired by other plugins during runtime building to add
/// more nodes to the render graph.
pub struct Rend3Plugin {
    pub iad: InstanceAdapterDevice,
    pub renderer: Arc<Renderer>,
    pub base_render_graph: BaseRenderGraph,
    pub pbr_routine: PbrRoutine,
    pub tonemapping_routine: TonemappingRoutine,
    pub frame_request_rx: mpsc::UnboundedReceiver<FrameRequest>,
    pub frame_request_tx: mpsc::UnboundedSender<FrameRequest>,
}

#[async_trait]
impl Plugin for Rend3Plugin {
    fn build(&mut self, _builder: &mut RuntimeBuilder) {}

    async fn run(&mut self, _runtime: Arc<Runtime>) {
        while let Some(frame) = self.frame_request_rx.recv().await {
            self.draw(frame);
        }
    }
}

impl Rend3Plugin {
    /// Creates a new rend3 plugin from an existing [InstanceAdapterDevice] and
    /// the target window's texture format.
    pub fn new(iad: InstanceAdapterDevice, format: TextureFormat) -> Self {
        let handedness = rend3::types::Handedness::Right;
        let renderer = Renderer::new(iad.to_owned(), handedness, None).unwrap();
        let base_render_graph = BaseRenderGraph::new(&renderer);
        let mut data_core = renderer.data_core.lock();
        let interfaces = &base_render_graph.interfaces;
        let pbr_routine = PbrRoutine::new(&renderer, &mut data_core, interfaces);
        let tonemapping_routine = TonemappingRoutine::new(&renderer, interfaces, format);
        drop(data_core);

        let (frame_request_tx, frame_request_rx) = mpsc::unbounded_channel();

        Self {
            iad,
            renderer,
            base_render_graph,
            pbr_routine,
            tonemapping_routine,
            frame_request_tx,
            frame_request_rx,
        }
    }

    /// Draws a frame in response to a [FrameRequest].
    pub fn draw(&self, request: FrameRequest) {
        let (cmd_bufs, ready) = self.renderer.ready();
        let mut graph = RenderGraph::new();

        let aspect = request.resolution.as_vec2();
        let aspect = aspect.x / aspect.y;
        self.renderer.set_aspect_ratio(aspect);
        self.renderer.set_camera_data(request.camera);

        self.base_render_graph.add_to_graph(
            &mut graph,
            &ready,
            &self.pbr_routine,
            None,
            &self.tonemapping_routine,
            request.resolution,
            SampleCount::One,
            glam::Vec4::ZERO,
        );

        graph.execute(&self.renderer, request.output_frame, cmd_bufs, &ready);

        let _ = request.on_complete.send(()); // ignore hangup
    }
}

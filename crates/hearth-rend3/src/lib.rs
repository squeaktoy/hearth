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
use rend3::{InstanceAdapterDevice, Renderer};
use rend3_routine::base::BaseRenderGraph;
use rend3_routine::pbr::PbrRoutine;
use rend3_routine::tonemapping::TonemappingRoutine;
use wgpu::TextureFormat;

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
}

#[async_trait]
impl Plugin for Rend3Plugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {}

    async fn run(&mut self, runtime: Arc<Runtime>) {}
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

        Self {
            iad,
            renderer,
            base_render_graph,
            pbr_routine,
            tonemapping_routine,
        }
    }
}

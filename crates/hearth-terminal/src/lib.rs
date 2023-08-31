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

use hearth_core::runtime::{Plugin, RuntimeBuilder};
use hearth_rend3::*;
use rend3_alacritty::{
    terminal::{Terminal, TerminalConfig, TerminalState},
    text::{FaceAtlas, FontSet},
    *,
};
use rend3_routine::base::BaseRenderGraphIntermediateState;

pub struct TerminalRoutine {
    config: TerminalConfig,
    store: TerminalStore,
}

impl TerminalRoutine {
    pub fn new(rend3: &Rend3Plugin) -> Self {
        let ttf_srcs = FontSet {
            regular: include_bytes!("../../../resources/mononoki/mononoki-Regular.ttf").to_vec(),
            italic: include_bytes!("../../../resources/mononoki/mononoki-Italic.ttf").to_vec(),
            bold: include_bytes!("../../../resources/mononoki/mononoki-Bold.ttf").to_vec(),
            bold_italic: include_bytes!("../../../resources/mononoki/mononoki-BoldItalic.ttf")
                .to_vec(),
        };

        let fonts = ttf_srcs.map(|src| {
            let face = owned_ttf_parser::OwnedFace::from_vec(src, 0).unwrap();

            let face_atlas = FaceAtlas::new(
                face,
                &rend3.renderer.device,
                rend3.renderer.queue.to_owned(),
            );

            Arc::new(face_atlas)
        });

        let command = None;
        let config = TerminalConfig { fonts, command };

        let mut store = TerminalStore::new(config.clone(), &rend3.renderer, rend3.surface_format);

        // debug terminal
        let state = TerminalState::default();
        let terminal = Terminal::new(config.clone(), state);
        store.insert_terminal(&terminal);

        Self { store, config }
    }
}

impl Routine for TerminalRoutine {
    fn build_node(&mut self) -> Box<dyn Node + '_> {
        Box::new(TerminalNode {
            routine: self.store.create_routine(),
        })
    }
}

pub struct TerminalNode<'a> {
    routine: TerminalRenderRoutine<'a>,
}

impl<'a> Node<'a> for TerminalNode<'a> {
    fn draw<'graph>(&'graph self, info: &mut RoutineInfo<'_, 'graph>) {
        let state = BaseRenderGraphIntermediateState::new(
            info.graph,
            info.ready_data,
            info.resolution,
            info.sample_count,
        );

        let output = info.graph.add_surface_texture();
        let depth = state.depth;
        self.routine.add_to_graph(info.graph, output, depth);
    }
}

pub struct TerminalPlugin {}

impl Plugin for TerminalPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        let rend3 = builder
            .get_plugin_mut::<Rend3Plugin>()
            .expect("rend3 plugin was not found");

        let routine = TerminalRoutine::new(rend3);

        rend3.add_routine(routine);
    }
}

impl TerminalPlugin {
    pub fn new() -> Self {
        Self {}
    }
}

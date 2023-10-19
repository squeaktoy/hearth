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

use hearth_core::{
    async_trait,
    process::factory::ProcessInfo,
    runtime::{Plugin, RuntimeBuilder},
    tokio::{
        self,
        sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    },
    utils::{
        ProcessRunner, RequestInfo, RequestResponseProcess, ResponseInfo, ServiceRunner,
        SinkProcess,
    },
};
use hearth_rend3::*;
use hearth_types::{terminal::*, Flags};
use rend3_alacritty::{
    terminal::{Terminal, TerminalConfig, TerminalState},
    text::{FaceAtlas, FontSet},
    *,
};
use rend3_routine::base::BaseRenderGraphIntermediateState;

pub use rend3_alacritty;

pub struct TerminalRoutine {
    store: TerminalStore,
    new_terminals: UnboundedReceiver<Arc<Terminal>>,
}

impl TerminalRoutine {
    pub fn new(
        config: TerminalConfig,
        rend3: &Rend3Plugin,
        new_terminals: UnboundedReceiver<Arc<Terminal>>,
    ) -> Self {
        Self {
            store: TerminalStore::new(config, &rend3.renderer, rend3.surface_format),
            new_terminals,
        }
    }
}

impl Routine for TerminalRoutine {
    fn build_node(&mut self) -> Box<dyn Node + '_> {
        while let Ok(terminal) = self.new_terminals.try_recv() {
            self.store.insert_terminal(&terminal);
        }

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

/// Converts a serialized `TerminalState` into a `rend3-alacritty`-friendly [TerminalState].
pub fn convert_state(state: &hearth_types::terminal::TerminalState) -> TerminalState {
    TerminalState {
        position: state.position,
        orientation: state.orientation,
        half_size: state.half_size,
        opacity: state.opacity,
        padding: state.padding,
        units_per_em: state.units_per_em,
        ..Default::default()
    }
}

/// Guest-exposed terminal process.
pub struct TerminalSink {
    inner: Option<Arc<Terminal>>,
}

#[async_trait]
impl SinkProcess for TerminalSink {
    type Message = TerminalUpdate;

    async fn on_message(&mut self, request: &mut RequestInfo<'_, Self::Message>) {
        let Some(inner) = self.inner.as_ref() else {
            return;
        };

        match &request.data {
            TerminalUpdate::Quit => {
                self.inner.take();
            }
            TerminalUpdate::Input(input) => {
                inner.send_input(input);
            }
            TerminalUpdate::State(state) => {
                inner.update(convert_state(state));
            }
        }
    }
}

/// Guest-exposed service plugin.
pub struct TerminalFactory {
    config: TerminalConfig,
    new_terminals_tx: UnboundedSender<Arc<Terminal>>,
}

#[async_trait]
impl RequestResponseProcess for TerminalFactory {
    type Request = FactoryRequest;
    type Response = FactoryResponse;

    async fn on_request(
        &mut self,
        request: &mut RequestInfo<'_, Self::Request>,
    ) -> ResponseInfo<Self::Response> {
        let FactoryRequest::CreateTerminal(state) = &request.data;

        let state = convert_state(state);
        let terminal = Terminal::new(self.config.clone(), state);
        let _ = self.new_terminals_tx.send(terminal.clone());

        let sink = TerminalSink {
            inner: Some(terminal),
        };

        let info = ProcessInfo {};
        let flags = Flags::SEND | Flags::KILL;
        let child = request.runtime.process_factory.spawn(info, flags);
        let child_cap = request.ctx.copy_self_capability(&child);

        tokio::spawn(sink.run("TerminalSink".to_string(), request.runtime.clone(), child));

        ResponseInfo {
            data: Ok(FactorySuccess::Terminal),
            caps: vec![child_cap],
        }
    }
}

impl ServiceRunner for TerminalFactory {
    const NAME: &'static str = "hearth.terminal.TerminalFactory";
}

pub struct TerminalPlugin {}

impl Plugin for TerminalPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        let rend3 = builder
            .get_plugin_mut::<Rend3Plugin>()
            .expect("rend3 plugin was not found");

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

        let (new_terminals_tx, new_terminals) = unbounded_channel();

        rend3.add_routine(TerminalRoutine::new(config.clone(), rend3, new_terminals));

        builder.add_plugin(TerminalFactory {
            config,
            new_terminals_tx,
        });
    }
}

impl TerminalPlugin {
    pub fn new() -> Self {
        Self {}
    }
}

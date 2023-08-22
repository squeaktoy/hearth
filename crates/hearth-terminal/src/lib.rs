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

use std::sync::{
    mpsc::{channel, Sender},
    Arc,
};

use alacritty_terminal::{
    config::PtyConfig,
    event::{Event as TermEvent, EventListener},
    event_loop::{EventLoop as TermEventLoop, Msg as TermMsg, State as TermState},
    sync::FairMutex,
    term::color::{Colors, Rgb},
    tty::Pty,
    Term,
};
use hearth_core::runtime::{Plugin, RuntimeBuilder};
use hearth_rend3::*;
use rend3_alacritty::*;
use rend3_routine::base::BaseRenderGraphIntermediateState;

pub struct TermListener {
    sender: Sender<TermEvent>,
}

impl TermListener {
    pub fn new(sender: Sender<TermEvent>) -> Self {
        Self { sender }
    }
}

impl EventListener for TermListener {
    fn send_event(&self, event: TermEvent) {
        self.sender.send(event).unwrap();
    }
}

pub struct TerminalRoutine {
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

        let config = Arc::new(TerminalConfig { fonts });

        let store = TerminalStore::new(config, &rend3.renderer, rend3.surface_format);

        Self { store }
    }

    pub fn add_terminal(&mut self) {
        let term_size =
            alacritty_terminal::term::SizeInfo::new(100.0, 75.0, 1.0, 1.0, 0.0, 0.0, false);

        let (sender, term_events) = channel();

        let shell = alacritty_terminal::config::Program::Just("/usr/bin/fish".into());

        let term_config = alacritty_terminal::config::Config {
            pty_config: PtyConfig {
                shell: Some(shell),
                working_directory: None,
                hold: false,
            },
            ..Default::default()
        };

        let term_listener = TermListener::new(sender.clone());

        let term = Term::new(&term_config, term_size, term_listener);
        let term = FairMutex::new(term);
        let term = Arc::new(term);

        let pty = alacritty_terminal::tty::new(&term_config.pty_config, &term_size, None).unwrap();

        let term_listener = TermListener::new(sender);
        let term_loop = TermEventLoop::new(term.clone(), term_listener, pty, false, false);
        let term_channel = term_loop.channel();

        let mut colors = Colors::default();
        Self::load_colors(&mut colors);
    }

    pub fn load_colors(color: &mut Colors) {
        use alacritty_terminal::ansi::NamedColor::*;

        let maps = [
            (Black, Rgb { r: 0, g: 0, b: 0 }),
            (Red, Rgb { r: 255, g: 0, b: 0 }),
            (Green, Rgb { r: 0, g: 255, b: 0 }),
            (Blue, Rgb { r: 0, g: 0, b: 255 }),
            (
                Yellow,
                Rgb {
                    r: 255,
                    g: 255,
                    b: 0,
                },
            ),
            (
                Magenta,
                Rgb {
                    r: 255,
                    g: 0,
                    b: 255,
                },
            ),
            (
                Cyan,
                Rgb {
                    r: 0,
                    g: 255,
                    b: 255,
                },
            ),
            (
                White,
                Rgb {
                    r: 255,
                    g: 255,
                    b: 255,
                },
            ),
        ];

        for map in maps.iter() {
            color[map.0] = Some(map.1);
        }

        let dupes = [
            (Background, Black),
            (Foreground, White),
            (BrightBlack, Black),
            (BrightRed, Red),
            (BrightGreen, Green),
            (BrightYellow, Yellow),
            (BrightBlue, Blue),
            (BrightMagenta, Magenta),
            (BrightCyan, Cyan),
            (BrightWhite, White),
        ];

        for (dst, src) in dupes.iter() {
            color[*dst] = color[*src];
        }
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

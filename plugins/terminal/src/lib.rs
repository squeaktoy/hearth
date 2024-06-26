use std::sync::Arc;

use draw::{TerminalDrawState, TerminalPipelines};
use hearth_rend3::*;
use hearth_runtime::{
    async_trait,
    hearth_macros::GetProcessMetadata,
    runtime::{Plugin, RuntimeBuilder},
    tokio::sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
    utils::*,
};
use hearth_schema::terminal::*;
use terminal::{Terminal, TerminalConfig};
use text::{FaceAtlas, FontSet};

/// Terminal rendering code.
pub mod draw;

/// Integration with `alacritty_terminal`.
pub mod terminal;

/// Low-level text and font helpers.
pub mod text;

/// Contains a terminal and its cached draw state.
pub struct TerminalWrapper {
    terminal: Arc<Terminal>,
    draw_state: TerminalDrawState,
}

impl TerminalWrapper {
    /// Updates this terminal's draw state. Returns true if this terminal has not quit.
    pub fn update(&mut self, pipelines: &TerminalPipelines) -> bool {
        let quit = self.terminal.should_quit();

        if !quit {
            self.terminal
                .update_draw_state(pipelines, &mut self.draw_state);
        }

        !quit
    }
}

pub struct TerminalRoutine {
    pipelines: TerminalPipelines,
    terminals: Vec<TerminalWrapper>,
    new_terminals: UnboundedReceiver<Arc<Terminal>>,
}

impl TerminalRoutine {
    pub fn new(rend3: &Rend3Plugin, new_terminals: UnboundedReceiver<Arc<Terminal>>) -> Self {
        Self {
            pipelines: TerminalPipelines::new(
                rend3.renderer.device.to_owned(),
                rend3.renderer.queue.to_owned(),
                rend3.surface_format,
            ),
            terminals: vec![],
            new_terminals,
        }
    }
}

impl Routine for TerminalRoutine {
    fn build_node(&mut self) -> Box<dyn Node + '_> {
        while let Ok(terminal) = self.new_terminals.try_recv() {
            self.terminals.push(TerminalWrapper {
                draw_state: TerminalDrawState::new(&self.pipelines, terminal.get_fonts()),
                terminal,
            });
        }

        // update draw states and remove terminals that have quit
        self.terminals.retain_mut(|t| t.update(&self.pipelines));

        Box::new(TerminalNode {
            pipelines: &self.pipelines,
            draws: self.terminals.iter().map(|term| &term.draw_state).collect(),
        })
    }
}

pub struct TerminalNode<'a> {
    pipelines: &'a TerminalPipelines,
    draws: Vec<&'a TerminalDrawState>,
}

impl<'a> Node<'a> for TerminalNode<'a> {
    fn draw<'graph>(&'graph self, info: &mut RoutineInfo<'_, 'graph>) {
        let output = info.graph.add_surface_texture();
        let depth = info.state.depth;
        self.pipelines
            .add_to_graph(self.draws.as_slice(), info.graph, output, depth);
    }
}

/// An instance of a terminal. Accepts TerminalUpdate.
#[derive(GetProcessMetadata)]
pub struct TerminalSink {
    inner: Arc<Terminal>,
}

impl Drop for TerminalSink {
    fn drop(&mut self) {
        self.inner.quit();
    }
}

#[async_trait]
impl SinkProcess for TerminalSink {
    type Message = TerminalUpdate;

    async fn on_message<'a>(&'a mut self, request: MessageInfo<'a, Self::Message>) {
        match request.data {
            TerminalUpdate::Quit => {
                self.inner.quit();
            }
            TerminalUpdate::Input(input) => {
                self.inner.send_input(&input);
            }
            TerminalUpdate::State(state) => {
                self.inner.update(state);
            }
        }
    }
}

/// The native terminal emulator factory service. Accepts FactoryRequest.
#[derive(GetProcessMetadata)]
pub struct TerminalFactory {
    fonts: FontSet<Arc<FaceAtlas>>,
    new_terminals_tx: UnboundedSender<Arc<Terminal>>,
}

#[async_trait]
impl RequestResponseProcess for TerminalFactory {
    type Request = FactoryRequest;
    type Response = FactoryResponse;

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response> {
        let FactoryRequest::CreateTerminal(state) = &request.data;

        let config = TerminalConfig {
            fonts: self.fonts.to_owned(),
            command: None,
        };

        let terminal = Terminal::new(config, state.clone());
        let _ = self.new_terminals_tx.send(terminal.clone());

        let child = request.spawn(TerminalSink { inner: terminal });

        ResponseInfo {
            data: Ok(FactorySuccess::Terminal),
            caps: vec![child],
        }
    }
}

impl ServiceRunner for TerminalFactory {
    const NAME: &'static str = "hearth.terminal.TerminalFactory";
}

#[derive(Default)]
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

        let (new_terminals_tx, new_terminals) = unbounded_channel();

        rend3.add_routine(TerminalRoutine::new(rend3, new_terminals));

        builder.add_plugin(TerminalFactory {
            fonts,
            new_terminals_tx,
        });
    }
}

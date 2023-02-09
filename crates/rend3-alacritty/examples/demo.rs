use alacritty_terminal::config::PtyConfig;
use alacritty_terminal::event::{Event as TermEvent, EventListener};
use alacritty_terminal::event_loop::{
    EventLoop as TermEventLoop, Msg as TermMsg, State as TermState,
};
use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::color::{Colors, Rgb};
use alacritty_terminal::tty::Pty;
use alacritty_terminal::Term;
use mio_extras::channel::Sender as MioSender;
use rend3_alacritty::AlacrittyRoutine;
use rend3_routine::base::BaseRenderGraphIntermediateState;
use winit::event::{Event, WindowEvent};
use winit::event_loop::ControlFlow;

use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

const SAMPLE_COUNT: rend3::types::SampleCount = rend3::types::SampleCount::One;

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

pub struct DemoInner {
    alacritty_routine: AlacrittyRoutine,
    term_loop: JoinHandle<(TermEventLoop<Pty, TermListener>, TermState)>,
    term_channel: MioSender<TermMsg>,
    term_events: Receiver<TermEvent>,
    term: Arc<FairMutex<Term<TermListener>>>,
    colors: Colors,
}

impl DemoInner {
    pub fn new(
        renderer: &Arc<rend3::Renderer>,
        surface_format: rend3::types::TextureFormat,
    ) -> Self {
        let ttf_src = include_bytes!("../../../resources/mononoki/mononoki-Regular.ttf");
        let ttf_src = ttf_src.to_vec();
        let face = owned_ttf_parser::OwnedFace::from_vec(ttf_src, 0).unwrap();
        let alacritty_routine = AlacrittyRoutine::new(face, &renderer, surface_format);

        let term_size =
            alacritty_terminal::term::SizeInfo::new(180.0, 80.0, 1.0, 1.0, 0.0, 0.0, false);

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

        Self {
            alacritty_routine,
            term,
            term_loop: term_loop.spawn(),
            term_channel,
            term_events,
            colors,
        }
    }

    pub fn load_colors(color: &mut Colors) {
        use alacritty_terminal::ansi::NamedColor::*;
        use alacritty_terminal::term::color::Rgb;

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

    pub fn virtual_keycode_to_string(
        keycode: winit::event::VirtualKeyCode,
    ) -> Option<&'static str> {
        use winit::event::VirtualKeyCode::*;
        match keycode {
            Back => Some("\x7f"),
            Up => Some("\x1b[A"),
            Down => Some("\x1b[B"),
            Right => Some("\x1b[C"),
            Left => Some("\x1b[D"),
            Home => Some("\x1b[1~"),
            Insert => Some("\x1b[2~"),
            Delete => Some("\x1b[3~"),
            End => Some("\x1b[4~"),
            PageUp => Some("\x1b[5~"),
            PageDown => Some("\x1b[6~"),
            _ => None,
        }
    }

    pub fn on_keyboard_input(&mut self, input: &winit::event::KeyboardInput) {
        if input.state == winit::event::ElementState::Pressed {
            if let Some(keycode) = input.virtual_keycode {
                if let Some(input) = Self::virtual_keycode_to_string(keycode) {
                    self.send_input(input);
                }
            }
        }
    }

    pub fn on_received_character(&mut self, c: char) {
        match c {
            '\u{7f}' | '\u{8}' => {
                // We use a special escape code for the delete and backspace keys.
                return;
            }
            _ => {}
        }

        let string = c.to_string();
        self.send_input(string.as_str());
    }

    pub fn send_input(&mut self, input: &str) {
        let bytes = input.as_bytes();
        let cow = std::borrow::Cow::Owned(bytes.to_owned().into());
        self.term_channel.send(TermMsg::Input(cow)).unwrap();
    }
}

#[derive(Default)]
pub struct Demo {
    inner: Option<DemoInner>,
}

impl rend3_framework::App for Demo {
    const HANDEDNESS: rend3::types::Handedness = rend3::types::Handedness::Right;

    fn sample_count(&self) -> rend3::types::SampleCount {
        SAMPLE_COUNT
    }

    fn setup(
        &mut self,
        _window: &winit::window::Window,
        renderer: &Arc<rend3::Renderer>,
        _routines: &Arc<rend3_framework::DefaultRoutines>,
        surface_format: rend3::types::TextureFormat,
    ) {
        self.inner = Some(DemoInner::new(renderer, surface_format));
    }

    fn handle_event(
        &mut self,
        window: &winit::window::Window,
        renderer: &Arc<rend3::Renderer>,
        routines: &Arc<rend3_framework::DefaultRoutines>,
        base_rendergraph: &rend3_routine::base::BaseRenderGraph,
        surface: Option<&Arc<rend3::types::Surface>>,
        resolution: glam::UVec2,
        event: rend3_framework::Event<'_, ()>,
        control_flow: impl FnOnce(winit::event_loop::ControlFlow),
    ) {
        let inner = self.inner.as_mut().unwrap();
        match event {
            Event::WindowEvent { event, .. } => match event {
                WindowEvent::CloseRequested => {
                    control_flow(ControlFlow::Exit);
                }

                WindowEvent::KeyboardInput { input, .. } => {
                    inner.on_keyboard_input(&input);
                }
                WindowEvent::ReceivedCharacter(c) => {
                    inner.on_received_character(c);
                }
                _ => {}
            },
            Event::MainEventsCleared => {
                while let Ok(event) = inner.term_events.try_recv() {
                    match event {
                        TermEvent::ColorRequest(index, format) => {
                            let color = inner.colors[index].unwrap_or(Rgb {
                                r: 255,
                                g: 0,
                                b: 255,
                            });

                            inner.send_input(&format(color));
                        }
                        TermEvent::PtyWrite(text) => inner.send_input(&text),
                        TermEvent::Exit => {
                            control_flow(ControlFlow::Exit);
                            return;
                        }
                        _ => {}
                    }
                }

                window.request_redraw();
            }
            Event::RedrawRequested(_) => {
                let frame = rend3::util::output::OutputFrame::Surface {
                    surface: Arc::clone(surface.unwrap()),
                };

                let (cmd_bufs, ready) = renderer.ready();

                let pbr_routine = rend3_framework::lock(&routines.pbr);
                let tonemapping_routine = rend3_framework::lock(&routines.tonemapping);
                let mut graph = rend3::graph::RenderGraph::new();

                base_rendergraph.add_to_graph(
                    &mut graph,
                    &ready,
                    &pbr_routine,
                    None,
                    &tonemapping_routine,
                    resolution,
                    SAMPLE_COUNT,
                    glam::Vec4::ZERO,
                );

                let state = BaseRenderGraphIntermediateState::new(
                    &mut graph,
                    &ready,
                    resolution,
                    SAMPLE_COUNT,
                );

                let depth = state.depth;
                let output = graph.add_surface_texture();

                let term = inner.term.lock();
                inner.alacritty_routine.update(&term, &inner.colors);
                inner
                    .alacritty_routine
                    .add_to_graph(&mut graph, output, depth);

                graph.execute(renderer, frame, cmd_bufs, &ready);
            }
            _ => {}
        }
    }
}

fn main() {
    let app = Demo::default();
    rend3_framework::start(
        app,
        winit::window::WindowBuilder::new().with_title("rend3-alacritty demo"),
    );
}

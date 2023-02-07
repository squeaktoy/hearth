use tokio::sync::{mpsc, oneshot};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop, EventLoopBuilder, EventLoopProxy};
use winit::window::{Window, WindowBuilder};

/// A message sent from the rest of the program to a window.
#[derive(Clone, Debug)]
pub enum WindowRxMessage {
    /// The window is requested to quit.
    Quit,
}

/// A message sent from a window to the rest of the program.
#[derive(Clone, Debug)]
pub enum WindowTxMessage {
    /// The window has been requested to quit.
    Quit,
}

/// Message sent from the window on initialization.
#[derive(Debug)]
pub struct WindowOffer {
    pub event_rx: EventLoopProxy<WindowRxMessage>,
    pub event_tx: mpsc::UnboundedReceiver<WindowTxMessage>,
}

pub struct WindowCtx {
    event_loop: EventLoop<WindowRxMessage>,
    event_tx: mpsc::UnboundedSender<WindowTxMessage>,
    window: Window,
}

impl WindowCtx {
    pub fn new(offer_sender: oneshot::Sender<WindowOffer>) -> Self {
        let event_loop = EventLoopBuilder::with_user_event().build();
        let proxy = event_loop.create_proxy();
        let window = WindowBuilder::new()
            .with_title("Hearth Client")
            .with_inner_size(winit::dpi::LogicalSize::new(128.0, 128.0))
            .build(&event_loop)
            .unwrap();

        let (event_rx, event_tx) = mpsc::unbounded_channel();

        offer_sender
            .send(WindowOffer {
                event_rx: proxy,
                event_tx,
            })
            .unwrap();

        Self {
            event_loop,
            event_tx: event_rx,
            window,
        }
    }

    pub fn run(self) -> ! {
        let Self {
            event_loop,
            event_tx,
            window,
        } = self;

        event_loop.run(move |event, _, control_flow| {
            *control_flow = ControlFlow::Wait;

            match &event {
                Event::WindowEvent {
                    event: WindowEvent::CloseRequested,
                    ..
                } => {
                    *control_flow = ControlFlow::Exit;
                    event_tx.send(WindowTxMessage::Quit).unwrap();
                }
                Event::MainEventsCleared => {
                    window.request_redraw();
                }
                Event::UserEvent(WindowRxMessage::Quit) => {
                    *control_flow = ControlFlow::Exit;
                }
                _ => (),
            }
        });
    }
}

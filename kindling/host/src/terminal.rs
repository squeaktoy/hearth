use super::*;

use hearth_guest::terminal::*;

lazy_static::lazy_static! {
    static ref TERMINAL_FACTORY: RequestResponse<FactoryRequest, FactoryResponse> =
        RequestResponse::expect_service("hearth.terminal.TerminalFactory");
}

/// A wrapper around the Terminal Capability.
pub struct Terminal {
    cap: Capability,
}

impl Drop for Terminal {
    fn drop(&mut self) {
        self.cap.send(&TerminalUpdate::Quit, &[]);
    }
}

impl Terminal {
    /// Creates a new terminal with the given TerminalState.
    ///
    /// Panics if the factory responds with an error.
    pub fn new(state: TerminalState) -> Self {
        let resp = TERMINAL_FACTORY.request(FactoryRequest::CreateTerminal(state), &[]);
        let _ = resp.0.unwrap();
        Terminal {
            cap: resp.1.get(0).unwrap().clone(),
        }
    }

    /// Send input to this terminal.
    pub fn input(&self, input: String) {
        self.cap.send(&TerminalUpdate::Input(input), &[])
    }

    /// Update the state of this terminal.
    pub fn update(&self, state: TerminalState) {
        self.cap.send(&TerminalUpdate::State(state), &[])
    }
}

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

use super::*;

use hearth_guest::terminal::*;

lazy_static::lazy_static! {
    /// A lazily-initialized handle to the terminal factory service.
    pub static ref TERMINAL_FACTORY: RequestResponse<FactoryRequest, FactoryResponse> = {
        RequestResponse::new(registry::REGISTRY.get_service("hearth.canvas.TerminalFactory").unwrap())    };
}

impl TERMINAL_FACTORY {
    /// Spawns a new Terminal.
    pub fn spawn_canvas(&self, request: FactoryRequest) -> TerminalWrapper {
        let resp = self.request(request, &[]);
        let success = resp.0.unwrap();
        match success {
            FactorySuccess::Terminal => TerminalWrapper {
                cap: resp.1.get(0).unwrap().clone(),
            },
        }
    }
}

/// A wrapper around the Terminal Capability.
pub struct TerminalWrapper {
    cap: Capability,
}

// This may change, depends on https://github.com/hearth-rs/hearth/issues/189
impl Drop for TerminalWrapper {
    fn drop(&mut self) {
        self.cap.send_json(&TerminalUpdate::Quit, &[]);
    }
}

impl TerminalWrapper {
    /// Send input to this terminal
    pub fn input(&self, input: String) {
        self.cap.send_json(&TerminalUpdate::Input(input), &[])
    }

    /// Update the state of this terminal.
    pub fn state(&self, state: TerminalState) {
        self.cap.send_json(&TerminalUpdate::State(state), &[])
    }
}

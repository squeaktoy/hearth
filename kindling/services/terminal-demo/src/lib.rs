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

use hearth_guest::{terminal::*, *};

/// Type alias for the native terminal factory service.
pub type TerminalFactory = RequestResponse<FactoryRequest, FactoryResponse>;

#[no_mangle]
pub extern "C" fn run() {
    // retrieve the native terminal factory service and wrap it in TerminalFactory
    let tf = TerminalFactory::new(
        REGISTRY
            .get_service("hearth.terminal.TerminalFactory")
            .expect("terminal factory service unavailable"),
    );

    // spawn each terminal using the terminal factory and a select palette
    spawn_terminal(&tf, 0, 0, Palette::rose_pine());
    spawn_terminal(&tf, 0, 1, Palette::gruvbox_material());
    spawn_terminal(&tf, 1, 0, Palette::solarized_dark());
    spawn_terminal(&tf, 1, 1, Palette::pretty_in_pink());
}

fn spawn_terminal(tf: &TerminalFactory, x: i32, y: i32, palette: Palette) {
    // create hashmap of 8-bit colors from palette
    let colors = FromIterator::from_iter([
        (0x0, palette.black),   // black
        (0x1, palette.red),     // red
        (0x2, palette.green),   // green
        (0x3, palette.yellow),  // yellow
        (0x4, palette.blue),    // blue
        (0x5, palette.magenta), // magenta
        (0x6, palette.cyan),    // cyan
        (0x7, palette.white),   // white
        (0x8, palette.black),   // bright black
        (0x9, palette.red),     // bright red
        (0xA, palette.green),   // bright green
        (0xB, palette.yellow),  // bright yellow
        (0xC, palette.blue),    // bright blue
        (0xD, palette.magenta), // bright magenta
        (0xE, palette.cyan),    // bright cyan
        (0xF, palette.white),   // bright white
        (0x100, palette.fg),    // foreground
        (0x101, palette.bg),    // background
    ]);

    // spawn a terminal
    let request = FactoryRequest::CreateTerminal(TerminalState {
        // reasonable translation on a grid
        position: (x as f32 * 2.8 - 1.4, y as f32 * 2.8 - 1.4, 0.0).into(),
        // face the default direction
        orientation: Default::default(),
        // size the terminals with margins
        half_size: (1.25, 1.25).into(),
        // opaque background
        opacity: 1.0,
        // no padding
        padding: Default::default(),
        // 6cm per glyph em
        units_per_em: 0.06,
        // given palette
        colors,
    });

    // send the spawn request
    let (msg, mut caps) = tf.request(request, &[]);

    // assert that it worked
    msg.unwrap();

    // hacky way to wait for the shell to start up so that pipes actually starts
    for _ in 0..10_000 {
        let _ = REGISTRY
            .get_service("hearth.terminal.TerminalFactory")
            .unwrap();
    }

    // get a handle to the terminal returned by the spawn response
    let term = caps.remove(0);

    // enter the pipes command
    term.send_json(&TerminalUpdate::Input("pipes\n".into()), &[]);
}

/// Helper struct for containing and identifying terminal colors.
struct Palette {
    pub bg: Color,
    pub fg: Color,
    pub black: Color,
    pub red: Color,
    pub green: Color,
    pub yellow: Color,
    pub blue: Color,
    pub magenta: Color,
    pub cyan: Color,
    pub white: Color,
}

/// Shorthand color initialization. Fixes alpha to 0xff.
fn c(rgb: u32) -> Color {
    Color(0xff000000 | rgb)
}

impl Palette {
    pub fn rose_pine() -> Self {
        Self {
            bg: c(0x191724),
            fg: c(0xe0def4),
            black: c(0x26233a),
            red: c(0xeb6f92),
            green: c(0x31748f),
            yellow: c(0xf6c177),
            blue: c(0x9ccfd8),
            magenta: c(0xc4a7e7),
            cyan: c(0xebbcba),
            white: c(0xe0def4),
        }
    }

    pub fn gruvbox_material() -> Self {
        Self {
            bg: c(0x1d2021),
            fg: c(0xd4be98),
            black: c(0x504945),
            red: c(0xea6962),
            green: c(0xa9b665),
            yellow: c(0xd8a657),
            blue: c(0x7daea3),
            magenta: c(0xd3869b),
            cyan: c(0x89b482),
            white: c(0xddc7a1),
        }
    }

    pub fn pretty_in_pink() -> Self {
        Self {
            bg: c(0x1e1a1d),
            fg: c(0xffccec),
            black: c(0x1e1e1e),
            red: c(0xf6084c),
            green: c(0x67ff6d),
            yellow: c(0xffc44e),
            blue: c(0x2593be),
            magenta: c(0xd68bff),
            cyan: c(0x00fafa),
            white: c(0xe0def4),
        }
    }

    pub fn solarized_dark() -> Self {
        Self {
            bg: c(0x002b36),
            fg: c(0x839496),
            black: c(0x073642),
            red: c(0xdc322f),
            green: c(0x859900),
            yellow: c(0xb58900),
            blue: c(0x268bd2),
            magenta: c(0xd33682),
            cyan: c(0x2aa198),
            white: c(0xeee8d5),
        }
    }
}

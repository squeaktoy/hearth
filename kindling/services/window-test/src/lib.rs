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

use hearth_guest::{window::*, *};
use kindling_host::registry::REGISTRY;

export_metadata!();

#[no_mangle]
pub extern "C" fn run() {
    let window = REGISTRY.get_service(SERVICE_NAME).unwrap();
    let events = Mailbox::new();
    let events_cap = events.make_capability(Permissions::SEND);

    window.send_json(&WindowCommand::Subscribe, &[&events_cap]);

    loop {
        let (msg, _) = events.recv_json::<WindowEvent>();

        if let WindowEvent::Redraw { .. } = msg {
            continue;
        }

        log(
            ProcessLogLevel::Info,
            "window-test",
            &format!("window event: {:?}", msg),
        );
    }
}

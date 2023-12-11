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

use core::panic;

use hearth_guest::{fs::*, log, Capability, Lump, LumpId, Mailbox, Permissions};

use crate::registry::REGISTRY;

pub struct Filesystem {
    cap: Capability,
}

impl Filesystem {
    pub fn new() -> Self {
        Filesystem {
            cap: REGISTRY
                .get_service("hearth.fs.Filesystem")
                .expect("Filesystem service not found"),
        }
    }

    fn request(&self, request: Request) -> Response {
        log(
            hearth_guest::ProcessLogLevel::Debug,
            core::module_path!(),
            &format!("making fs request {:?}", request),
        );
        let reply = Mailbox::new();
        let reply_cap = reply.make_capability(Permissions::SEND);
        self.cap.send_json(&request, &[&reply_cap]);
        // return only the response
        reply.recv_json::<Response>().0
    }

    pub fn get_file(&self, path: &str) -> Result<LumpId, Error> {
        let success = self.request(Request {
            target: path.to_string(),
            kind: RequestKind::Get,
        })?;
        match success {
            Success::Get(lump) => Ok(lump),
            _ => panic!("expected Success::Get, got {:?}", success),
        }
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>, Error> {
        let lump = self.get_file(path)?;
        let lump = Lump::load_by_id(&lump);
        Ok(lump.get_data())
    }

    pub fn list_files(&self, path: &str) -> Result<Vec<FileInfo>, Error> {
        let success = self.request(Request {
            target: path.to_string(),
            kind: RequestKind::List,
        })?;
        match success {
            Success::List(files) => Ok(files),
            _ => panic!("expected Success::List, got {:?}", success),
        }
    }
}

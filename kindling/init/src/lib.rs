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

use hearth_guest::*;

export_metadata!();

macro_rules! log {
    ($level:expr, $($arg:tt)*) => {
        ::hearth_guest::log(
            $level,
            ::core::module_path!(),
            &format!($($arg)*),
        )
    }
}

macro_rules! info {
    ($($arg:tt)*) => {
        log!(::hearth_guest::ProcessLogLevel::Info, $($arg)*);
    };
}

#[no_mangle]
pub extern "C" fn run() {
    hearth_guest::log(hearth_guest::ProcessLogLevel::Info, "init", "Hello world!");
    let fs = REGISTRY
        .get_service("hearth.fs.Filesystem")
        .expect("Filesystem service not found");
    let search_dir = "init";
    for file in list_files(&fs, search_dir) {
        info!("file: {}", file.name);
        let lump = get_file(&fs, &format!("init/{}/service.wasm", file.name));
        WASM_SPAWNER.request(
            wasm::WasmSpawnInfo {
                lump,
                entrypoint: None,
            },
            &[REGISTRY.as_ref()],
        );
    }
}

//TODO break these file system operations into a common crate
fn request_fs(fs: &Capability, request: fs::Request) -> fs::Success {
    log!(ProcessLogLevel::Debug, "making fs request: {:?}", request);
    let reply = Mailbox::new();
    let reply_cap = reply.make_capability(Permissions::SEND);
    fs.send_json(&request, &[&reply_cap]);
    let (response, _caps) = reply.recv_json::<fs::Response>();
    response.unwrap()
}

fn get_file(fs: &Capability, path: &str) -> LumpId {
    let success = request_fs(
        fs,
        fs::Request {
            target: path.to_string(),
            kind: fs::RequestKind::Get,
        },
    );

    let fs::Success::Get(lump) = success else {
        panic!("expected Success::Get, got {:?}", success)
    };

    lump
}

fn read_file(fs: &Capability, path: &str) -> Vec<u8> {
    let lump = get_file(fs, path);
    let lump = Lump::load_by_id(&lump);
    lump.get_data()
}

fn list_files(fs: &Capability, path: &str) -> Vec<fs::FileInfo> {
    let success = request_fs(
        fs,
        fs::Request {
            target: path.to_string(),
            kind: fs::RequestKind::List,
        },
    );

    let fs::Success::List(files) = success else {
        panic!("expected Success::List, got {:?}", success)
    };

    files
}

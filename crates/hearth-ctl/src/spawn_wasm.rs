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

use clap::Parser;
use hearth_ipc::RemoteProcess;
use hearth_rpc::hearth_types::wasm::WasmSpawnInfo;
use hearth_rpc::remoc::robj::lazy_blob::LazyBlob;
use hearth_rpc::*;
use hearth_types::*;
use std::fs::read;
use std::path::Path;
use yacexits::{EX_NOINPUT, EX_PROTOCOL, EX_UNAVAILABLE};

use crate::*;

/// Spawns a Web Assembly module on a specific peer
#[derive(Debug, Parser)]
pub struct SpawnWasm {
    #[clap(short, long)]
    pub peer: Option<u32>,
    pub file: String,
}

impl SpawnWasm {
    pub async fn run(self, daemon: DaemonOffer) -> CommandResult<()> {
        let peer_id = self.peer.map(|x| PeerId(x)).unwrap_or(daemon.peer_id);
        let mut ctx = PeerContext::new(&daemon, peer_id);
        let path = Path::new(&self.file);

        let pid = ctx
            .get_service_list()
            .await?
            .get("hearth.cognito.WasmProcessSpawner")
            .to_command_error("WasmProcessSpawner not found", EX_UNAVAILABLE)?
            .clone();
        let process = RemoteProcess::new(&daemon, ProcessInfo {}).await.unwrap();
        let lump_id = ctx
            .get_lump_store()
            .await?
            .upload_lump(
                None,
                LazyBlob::new(
                    read(path)
                        .to_command_error("reading wasm file", EX_NOINPUT)?
                        .into(),
                ),
            )
            .await
            .to_command_error("uploading lump", EX_PROTOCOL)?;

        let wasm_spawn_info = WasmSpawnInfo {
            lump: lump_id,
            entrypoint: None,
        };

        process
            .outgoing
            .send(Message {
                pid: ProcessId::from_peer_process(peer_id, pid),
                data: serde_json::to_vec(&wasm_spawn_info).unwrap(),
            })
            .await
            .to_command_error("sending message", EX_PROTOCOL)?;

        // necessary to flush the message send; remove when waiting for the returned PID
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        Ok(())
    }
}

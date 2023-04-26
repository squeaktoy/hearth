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

use std::{collections::HashMap, fmt::Display};

use clap::{Parser, Subcommand};
use hearth_rpc::{hearth_types::*, *};
use std::process::exit;

mod kill;
mod list_peers;
mod list_processes;
mod run_mock_runtime;
mod spawn_wasm;
use yacexits::{EX_OK, EX_PROTOCOL};

pub struct CommandError {
    message: String,
    exit_code: u32,
}

trait ToCommandError<T, E> {
    fn to_command_error<C: Display>(self, context: C, exit_code: u32) -> Result<T, CommandError>;
}

impl<T, E> ToCommandError<T, E> for Result<T, E>
where
    E: Display,
{
    fn to_command_error<C: Display>(self, context: C, exit_code: u32) -> Result<T, CommandError> {
        match self {
            Ok(ok) => Ok(ok),
            Err(e) => Err(CommandError {
                message: format!("{}: {}", context, e),
                exit_code,
            }),
        }
    }
}

impl<T> ToCommandError<T, ()> for Option<T> {
    fn to_command_error<C: Display>(self, context: C, exit_code: u32) -> Result<T, CommandError> {
        match self {
            Some(val) => Ok(val),
            None => Err(CommandError {
                message: context.to_string(),
                exit_code,
            }),
        }
    }
}

pub type CommandResult<T> = Result<T, CommandError>;

#[derive(Clone, Debug)]
pub enum MaybeLocalPid {
    Global(ProcessId),
    Local(LocalProcessId),
}

impl std::str::FromStr for MaybeLocalPid {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<ProcessId>() {
            Ok(pid) => Ok(MaybeLocalPid::Global(pid)),
            Err(_) => match s.parse::<u32>() {
                Ok(local_pid) => Ok(MaybeLocalPid::Local(LocalProcessId(local_pid))),
                Err(_) => Err("Failed to parse LocalPID or GlobalPID".into()),
            },
        }
    }
}

impl MaybeLocalPid {
    fn to_global_pid(&self, peer: PeerId) -> ProcessId {
        match self {
            MaybeLocalPid::Global(global_pid) => *global_pid,
            Self::Local(local_pid) => ProcessId::from_peer_process(peer, *local_pid),
        }
    }
}

pub struct PeerContext<'a> {
    offer: &'a DaemonOffer,
    peer_id: PeerId,
    peer_api: Option<PeerApiClient>,
    process_store: Option<ProcessStoreClient>,
    lump_store: Option<LumpStoreClient>,
}

impl<'a> PeerContext<'a> {
    pub fn new(offer: &'a DaemonOffer, peer_id: PeerId) -> Self {
        Self {
            offer,
            peer_id,
            peer_api: None,
            process_store: None,
            lump_store: None,
        }
    }

    async fn get_peer_api(&mut self) -> CommandResult<PeerApiClient> {
        if let Some(api) = self.peer_api.clone() {
            Ok(api)
        } else {
            let api = self
                .offer
                .peer_provider
                .find_peer(self.peer_id)
                .await
                .to_command_error("finding peer", EX_PROTOCOL)?;

            Ok(self.peer_api.insert(api).clone())
        }
    }

    async fn get_process_store(&mut self) -> CommandResult<ProcessStoreClient> {
        if let Some(store) = self.process_store.clone() {
            Ok(store)
        } else {
            let store = self
                .get_peer_api()
                .await?
                .get_process_store()
                .await
                .to_command_error("retrieving process store", EX_PROTOCOL)?;

            Ok(self.process_store.insert(store).clone())
        }
    }

    async fn get_process_list(&mut self) -> CommandResult<HashMap<LocalProcessId, ProcessStatus>> {
        self.get_process_store()
            .await?
            .follow_process_list()
            .await
            .to_command_error("following process list", EX_PROTOCOL)?
            .take_initial()
            .to_command_error("getting process list", EX_PROTOCOL)
    }

    async fn find_process(&mut self, local_pid: LocalProcessId) -> CommandResult<ProcessApiClient> {
        self.get_process_store()
            .await?
            .find_process(local_pid)
            .await
            .to_command_error("finding process", EX_PROTOCOL)
    }

    async fn get_lump_store(&mut self) -> CommandResult<LumpStoreClient> {
        if let Some(store) = self.lump_store.clone() {
            Ok(store)
        } else {
            let store = self
                .get_peer_api()
                .await?
                .get_lump_store()
                .await
                .to_command_error("retrieving lump store", EX_PROTOCOL)?;

            Ok(self.lump_store.insert(store).clone())
        }
    }

    async fn get_service_list(&mut self) -> CommandResult<HashMap<String, LocalProcessId>> {
        self.get_process_store()
            .await?
            .follow_service_list()
            .await
            .to_command_error("following service list", EX_PROTOCOL)?
            .take_initial()
            .to_command_error("getting service list", EX_PROTOCOL)
    }
}

/// Command-line interface (CLI) for interacting with a Hearth daemon over IPC.
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    ListPeers(list_peers::ListPeers),
    ListProcesses(list_processes::ListProcesses),
    RunMockRuntime(run_mock_runtime::RunMockRuntime),
    SpawnWasm(spawn_wasm::SpawnWasm),
    Kill(kill::Kill),
}

impl Commands {
    pub async fn run(self) -> CommandResult<()> {
        match self {
            Commands::ListPeers(args) => args.run(get_daemon().await?).await,
            Commands::ListProcesses(args) => args.run(get_daemon().await?).await,
            Commands::SpawnWasm(args) => args.run(get_daemon().await?).await,
            Commands::RunMockRuntime(args) => args.run().await,
            Commands::Kill(args) => args.run(get_daemon().await?).await,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    match args.command.run().await {
        Ok(_) => exit(EX_OK as i32),
        Err(e) => {
            eprintln!("ERROR: {}", e.message);
            exit(e.exit_code as i32)
        }
    }
}

async fn get_daemon() -> CommandResult<DaemonOffer> {
    hearth_ipc::connect()
        .await
        .to_command_error("connecting to Hearth daemon", EX_PROTOCOL)
}

fn hash_map_to_ordered_vec<K: Copy + Ord, V>(map: HashMap<K, V>) -> Vec<(K, V)> {
    let mut vec = map.into_iter().collect::<Vec<(K, V)>>();
    vec.sort_by_cached_key(|k| k.0);
    vec
}

async fn get_peer_list(daemon: &DaemonOffer) -> CommandResult<HashMap<PeerId, PeerInfo>> {
    daemon
        .peer_provider
        .follow_peer_list()
        .await
        .to_command_error("following peer lsit", EX_PROTOCOL)?
        .take_initial()
        .to_command_error("getting peer list", EX_PROTOCOL)
}

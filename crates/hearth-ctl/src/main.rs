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
use hearth_rpc::{hearth_types::*, DaemonOffer};
use std::process::exit;

mod kill;
mod list_peers;
mod list_processes;
mod run_mock_runtime;
mod spawn_wasm;

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

#[derive(Clone, Debug)]
pub enum MaybeLocalPID {
    Global(ProcessId),
    Local(LocalProcessId),
}

impl std::str::FromStr for MaybeLocalPID {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.parse::<ProcessId>() {
            Ok(pid) => Ok(MaybeLocalPID::Global(pid)),
            Err(_) => match s.parse::<u32>() {
                Ok(local_pid) => Ok(MaybeLocalPID::Local(LocalProcessId(local_pid))),
                Err(_) => Err("Failed to parse LocalPID or GlobalPID".into()),
            },
        }
    }
}

impl MaybeLocalPID {
    fn to_global_pid(&self, peer: PeerId) -> ProcessId {
        match self {
            MaybeLocalPID::Global(global_pid) => *global_pid,
            Self::Local(local_pid) => ProcessId::from_peer_process(peer, *local_pid),
        }
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
    pub async fn run(self) -> Result<(), CommandError> {
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
        Ok(_) => exit(0),
        Err(e) => {
            eprintln!("{}", e.message);
            exit(e.exit_code as i32)
        }
    }
}

async fn get_daemon() -> Result<DaemonOffer, CommandError> {
    hearth_ipc::connect()
        .await
        .to_command_error("connecting to Hearth daemon", yacexits::EX_UNAVAILABLE)
}

fn hash_map_to_ordered_vec<K: Copy + Ord, V>(map: HashMap<K, V>) -> Vec<(K, V)> {
    let mut vec = map.into_iter().collect::<Vec<(K, V)>>();
    vec.sort_by_cached_key(|k| k.0);
    vec
}

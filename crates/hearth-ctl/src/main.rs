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

use std::collections::HashMap;

use clap::{Parser, Subcommand};
use hearth_types::*;

pub struct DaemonOffer {}

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
    /// A dummy command.
    Dummy,
}

impl Commands {
    pub async fn run(self) {}
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    args.command.run().await;
}

async fn get_daemon() -> DaemonOffer {
    let _connection = hearth_ipc::connect()
        .await
        .expect("Failed to connect to Hearth daemon");

    DaemonOffer {}
}

fn hash_map_to_ordered_vec<K: Copy + Ord, V>(map: HashMap<K, V>) -> Vec<(K, V)> {
    let mut vec = map.into_iter().collect::<Vec<(K, V)>>();
    vec.sort_by_cached_key(|k| k.0);
    vec
}

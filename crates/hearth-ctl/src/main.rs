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
// Foobar is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Hearth. If not, see <https://www.gnu.org/licenses/>.

use clap::{Parser, Subcommand};
use hearth_rpc::DaemonOffer;

mod list_peers;

/// Command-line interface (CLI) for interacting with a Hearth daemon over IPC.
#[derive(Debug, Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Placeholder,
    ListPeers(list_peers::ListPeers),
}

impl Commands {
    pub async fn run(self, daemon: DaemonOffer) {
        match self {
            Commands::Placeholder => {}
            Commands::ListPeers(args) => args.run(daemon).await,
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    let daemon = hearth_ipc::connect()
        .await
        .expect("Failed to connect to Hearth daemon");
    args.command.run(daemon).await;
}

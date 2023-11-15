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
use hearth_rpc::*;
use hearth_schema::PeerId;

use crate::{get_peer_list, hash_map_to_ordered_vec, CommandResult, PeerContext};

/// Lists proccesses of either a singular peer or all peers in the space.
#[derive(Debug, Parser)]
pub struct ListProcesses {
    #[clap(short, long)]
    pub peer: Option<MaybeAllPeerId>,
}

#[derive(Debug, Clone)]
pub enum MaybeAllPeerId {
    All,
    One(PeerId),
}

impl std::str::FromStr for MaybeAllPeerId {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s.to_lowercase();
        match s.parse::<u32>() {
            Err(_) => {
                if s == "all" {
                    Ok(MaybeAllPeerId::All)
                } else {
                    Err("Bad peer ID".into())
                }
            }
            Ok(val) => Ok(MaybeAllPeerId::One(PeerId(val))),
        }
    }
}

impl ListProcesses {
    pub async fn run(self, daemon: DaemonOffer) -> CommandResult<()> {
        let peer_list = get_peer_list(&daemon).await?;

        match self.peer.unwrap_or(MaybeAllPeerId::One(daemon.peer_id)) {
            MaybeAllPeerId::All => {
                println!("{:>5} {:>5} {:<20}", "Peer", "PID", "Service");
                let mut is_first = true;
                for (id, _) in hash_map_to_ordered_vec(peer_list) {
                    if !is_first {
                        println!();
                    } else {
                        is_first = false;
                    }
                    Self::display_peer(PeerContext::new(&daemon, id)).await?;
                }
            }
            MaybeAllPeerId::One(id) => {
                println!("{:>5} {:>5} {:<20}", "Peer", "PID", "Service");
                Self::display_peer(PeerContext::new(&daemon, id)).await?;
            }
        }
        Ok(())
    }

    async fn display_peer(mut ctx: PeerContext<'_>) -> CommandResult<()> {
        let process_list = ctx.get_process_list().await?;
        let service_list = ctx.get_service_list().await?;

        // process info will need to be updated when fields are added to the struct
        for (process_id, _) in hash_map_to_ordered_vec(process_list) {
            print!("{:>5} ", ctx.peer_id.0);

            print!("{:>5} ", process_id.0);
            let mut is_first = true;
            for (service_name, service_id) in service_list.clone() {
                if service_id == process_id {
                    if is_first {
                        is_first = false;
                    } else {
                        print!(", ");
                    }

                    print!("{:<20}", service_name);
                }
            }
            println!();
        }
        Ok(())
    }
}

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
use hearth_types::PeerId;

/// Lists proccesses of either a singular peer or all peers in the space.
#[derive(Debug, Parser)]
pub struct ListProcesses {
    #[arg(short, long, default_value = None)]
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
    pub async fn run(self, daemon: DaemonOffer) {
        let peer_list = daemon
            .peer_provider
            .follow_peer_list()
            .await
            .unwrap()
            .take_initial()
            .unwrap();
        match self.peer.unwrap_or(MaybeAllPeerId::One(daemon.peer_id)) {
            MaybeAllPeerId::All => {
                println!("Peer ID\tPID\tService");
                for (id, _) in peer_list {
                    ListProcesses::display_peer(
                        daemon.peer_provider.find_peer(id).await.unwrap(),
                        Some(id),
                    )
                    .await;
                }
            }
            MaybeAllPeerId::One(id) => {
                println!("PID\tService");
                ListProcesses::display_peer(
                    daemon.peer_provider.find_peer(id).await.unwrap(),
                    None,
                )
                .await;
            }
        }
    }
    async fn display_peer(peer: PeerApiClient, peer_id: Option<PeerId>) {
        let process_store = peer.get_process_store().await.unwrap();
        let process_list = process_store
            .follow_process_list()
            .await
            .unwrap()
            .take_initial()
            .unwrap();
        let service_list = process_store
            .follow_service_list()
            .await
            .unwrap()
            .take_initial()
            .unwrap();

        // process info will need to be updated when fields are added to the struct
        for (process_id, _) in process_list {
            if peer_id.is_some() {
                print!("{}\t", peer_id.unwrap().0);
            }

            print!("{}\t", process_id.0);
            let mut is_first = true;
            for (service_name, service_id) in service_list.clone() {
                if service_id == process_id {
                    if is_first {
                        is_first = false;
                    } else {
                        print!(", ");
                    }

                    print!("{}", service_name);
                }
            }
            println!();
        }
    }
}

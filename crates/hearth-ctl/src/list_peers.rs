// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later

use clap::Parser;
use hearth_rpc::*;

/// Lists all peers currently participating in the space.
#[derive(Debug, Parser)]
pub struct ListPeers {}

impl ListPeers {
    pub async fn run(self, daemon: DaemonOffer) {
        let peer_list = daemon
            .peer_provider
            .follow_peer_list()
            .await
            .unwrap()
            .take_initial()
            .unwrap();

        //must be updated as time goes on when more peer info is added
        println!("PID\tNickname");
        for (peer_id, peer_info) in peer_list{
            print!("{}\t{}\n", peer_id.0, peer_info.nickname.unwrap_or(String::from("None")));
        }
        
    }
}

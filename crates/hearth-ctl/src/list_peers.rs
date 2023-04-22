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

use crate::hash_map_to_ordered_vec;

/// Lists all peers currently participating in the space.
#[derive(Debug, Parser)]
pub struct ListPeers {}

impl ListPeers {
    pub async fn run(self, daemon: DaemonOffer) {
        let mut peer_map = daemon
            .peer_provider
            .follow_peer_list()
            .await
            .unwrap()
            .take_initial()
            .unwrap();

        //must be updated as time goes on when more peer info is added
        println!("PID\tNickname");
        for (peer_id, peer_info) in hash_map_to_ordered_vec(peer_map) {
            print!(
                "{}\t{}\n",
                peer_id.0,
                peer_info.nickname.unwrap_or(String::from("None"))
            );
        }
    }
}

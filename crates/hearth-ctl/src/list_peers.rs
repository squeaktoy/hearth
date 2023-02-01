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

        eprintln!("{:#?}", peer_list);
    }
}

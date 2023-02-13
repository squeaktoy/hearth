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

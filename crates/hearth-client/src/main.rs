use std::net::SocketAddr;

use clap::Parser;
use hearth_network::auth::login;
use tokio::net::TcpStream;
use tracing::{debug, info, error};

/// Client program to the Hearth virtual space server.
#[derive(Parser, Debug)]
pub struct Args {
    /// IP address and port of the server to connect to.
    // TODO support DNS resolution too
    #[arg(short, long)]
    pub server: SocketAddr,

    /// Password to use to authenticate to the server. Defaults to empty.
    #[arg(short, long, default_value = "")]
    pub password: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let format = tracing_subscriber::fmt::format().compact();
    tracing_subscriber::fmt().event_format(format).init();

    info!("Connecting to server at {:?}...", args.server);
    let mut socket = match TcpStream::connect(args.server).await {
        Ok(s) => s,
        Err(err) => {
            error!("Failed to connect to server: {:?}", err);
            return;
        }
    };

    info!("Authenticating...");
    let key = match login(&mut socket, args.password.as_bytes()).await {
        Ok(key) => key,
        Err(err) => {
            error!("Failed to authenticate with server: {:?}", err);
            return;
        }
    };

    info!("Successfully connected!");
}

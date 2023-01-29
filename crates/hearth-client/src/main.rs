use std::net::SocketAddr;

use clap::Parser;
use hearth_network::auth::login;
use hearth_rpc::{ClientApiProvider, ClientApiProviderClient};
use tokio::net::TcpStream;
use tracing::{debug, error, info};

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
    let session_key = match login(&mut socket, args.password.as_bytes()).await {
        Ok(key) => key,
        Err(err) => {
            error!("Failed to authenticate with server: {:?}", err);
            return;
        }
    };

    use hearth_network::encryption::{AsyncDecryptor, AsyncEncryptor, Key};
    let client_key = Key::from_client_session(&session_key);
    let server_key = Key::from_server_session(&session_key);

    let (server_rx, server_tx) = tokio::io::split(socket);
    let server_rx = AsyncDecryptor::new(&server_key, server_rx);
    let server_tx = AsyncEncryptor::new(&client_key, server_tx);

    use remoc::rch::base::{Receiver, Sender};
    let cfg = remoc::Cfg::default();
    let (conn, _tx, mut rx): (_, Sender<()>, Receiver<ClientApiProviderClient>) =
        match remoc::Connect::io(cfg, server_rx, server_tx).await {
            Ok(v) => v,
            Err(err) => {
                error!("Remoc connection failure: {:?}", err);
                return;
            }
        };

    tokio::spawn(conn);

    let provider = rx.recv().await.unwrap().unwrap();

    info!("Successfully connected!");
}

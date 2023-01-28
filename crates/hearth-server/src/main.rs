use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use hearth_network::auth::ServerAuthenticator;
use tokio::net::{TcpListener, TcpStream};
use tracing::{error, info};

/// The Hearth virtual space server program.
#[derive(Parser, Debug)]
pub struct Args {
    /// IP address and port to listen on.
    #[arg(short, long)]
    pub bind: SocketAddr,

    /// Password to use to authenticate with clients. Defaults to empty.
    #[arg(short, long, default_value = "")]
    pub password: String,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    let format = tracing_subscriber::fmt::format().compact();
    tracing_subscriber::fmt().event_format(format).init();

    let authenticator = ServerAuthenticator::from_password(args.password.as_bytes()).unwrap();
    let authenticator = Arc::new(authenticator);

    info!("Binding to {:?}...", args.bind);
    let listener = match TcpListener::bind(args.bind).await {
        Ok(l) => l,
        Err(err) => {
            error!("Failed to listen: {:?}", err);
            return;
        }
    };

    info!("Listening");
    loop {
        let (socket, addr) = match listener.accept().await {
            Ok(v) => v,
            Err(err) => {
                error!("Listening error: {:?}", err);
                continue;
            }
        };

        info!("Connection from {:?}", addr);
        let authenticator = authenticator.clone();
        tokio::task::spawn(async move {
            on_accept(authenticator, socket, addr).await;
        });
    }
}

async fn on_accept(
    authenticator: Arc<ServerAuthenticator>,
    mut client: TcpStream,
    addr: SocketAddr,
) {
    info!("Authenticating with client {:?}...", addr);
    let session_key = match authenticator.login(&mut client).await {
        Ok(key) => key,
        Err(err) => {
            error!("Authentication error: {:?}", err);
            return;
        }
    };

    info!("Successfully authenticated");
}

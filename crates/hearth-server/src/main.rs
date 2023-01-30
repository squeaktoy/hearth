use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use hearth_network::auth::ServerAuthenticator;
use hearth_rpc::{
    CallResult, ClientApiProvider, ClientApiProviderClient, ClientApiProviderServerShared,
    ProcessApiClient,
};
use remoc::rtc::{async_trait, ServerShared};
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
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .event_format(format)
        .init();

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
    use hearth_network::encryption::{AsyncDecryptor, AsyncEncryptor, Key};
    let client_key = Key::from_client_session(&session_key);
    let server_key = Key::from_server_session(&session_key);

    let (client_rx, client_tx) = tokio::io::split(client);
    let client_rx = AsyncDecryptor::new(&client_key, client_rx);
    let client_tx = AsyncEncryptor::new(&server_key, client_tx);

    use remoc::rch::base::{Receiver, Sender};
    let cfg = remoc::Cfg::default();
    let (conn, mut tx, _rx): (_, Sender<ClientApiProviderClient>, Receiver<()>) =
        match remoc::Connect::io(cfg, client_rx, client_tx).await {
            Ok(v) => v,
            Err(err) => {
                error!("Remoc connection failure: {:?}", err);
                return;
            }
        };

    tokio::spawn(conn);

    let server = ClientApiProviderImpl;
    let server = std::sync::Arc::new(server);
    let (provider, provider_client) =
        ClientApiProviderServerShared::<_, remoc::codec::Default>::new(server, 1024);
    tokio::spawn(async move {
        provider.serve(true).await;
    });
    tx.send(provider_client).await.unwrap();
}

struct ClientApiProviderImpl;

#[async_trait]
impl ClientApiProvider for ClientApiProviderImpl {
    async fn get_process_api(&self) -> CallResult<ProcessApiClient> {
        error!("ClientApiProviderImpl::get_process_api() is unimplemented");
        Err(remoc::rtc::CallError::Dropped)
    }
}

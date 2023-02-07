use std::net::SocketAddr;

use clap::Parser;
use hearth_network::auth::login;
use hearth_rpc::*;
use tokio::net::TcpStream;
use tracing::{debug, error, info};

mod window;

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

fn main() {
    let args = Args::parse();
    hearth_core::init_logging();

    let (window_tx, window_rx) = tokio::sync::oneshot::channel();
    let window = window::WindowCtx::new(window_tx);

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();

    runtime.block_on(async {
        let mut window = window_rx.await.unwrap();
        let mut join_main = runtime.spawn(async move {
            async_main(args).await;
        });

        runtime.spawn(async move {
            loop {
                tokio::select! {
                    event = window.event_tx.recv() => {
                        debug!("window event: {:?}", event);
                    }
                    _ = &mut join_main => {
                        debug!("async_main joined");
                        window.event_rx.send_event(window::WindowRxMessage::Quit).unwrap();
                        break;
                    }
                }
            }
        });
    });

    debug!("Running window event loop");
    window.run();
}

async fn async_main(args: Args) {
    info!("Connecting to server at {:?}", args.server);
    let mut socket = match TcpStream::connect(args.server).await {
        Ok(s) => s,
        Err(err) => {
            error!("Failed to connect to server: {:?}", err);
            return;
        }
    };

    info!("Authenticating");
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
    let (conn, mut tx, mut rx): (_, Sender<ClientOffer>, Receiver<ServerOffer>) =
        match remoc::Connect::io(cfg, server_rx, server_tx).await {
            Ok(v) => v,
            Err(err) => {
                error!("Remoc connection failure: {:?}", err);
                return;
            }
        };

    debug!("Spawning Remoc connection thread");
    let join_connection = tokio::spawn(conn);

    debug!("Receiving server offer");
    let offer = rx.recv().await.unwrap().unwrap();

    info!("Assigned peer ID {:?}", offer.new_id);

    let peer_info = PeerInfo { nickname: None };
    let peer_api = hearth_core::api::spawn_peer_api(peer_info);

    tx.send(ClientOffer {
        peer_api: peer_api.to_owned(),
    })
    .await
    .unwrap();

    info!("Successfully connected!");

    debug!("Initializing IPC");
    let daemon_listener = match hearth_ipc::Listener::new().await {
        Ok(l) => l,
        Err(err) => {
            tracing::error!("IPC listener setup error: {:?}", err);
            return;
        }
    };

    let daemon_offer = DaemonOffer {
        peer_provider: offer.peer_provider.clone(),
        peer_id: offer.new_id,
    };

    hearth_ipc::listen(daemon_listener, daemon_offer);

    tokio::select! {
        result = join_connection => {
            result.unwrap().unwrap();
        }
        _ = hearth_core::wait_for_interrupt() => {
            info!("Ctrl+C hit; quitting client");
        }
    }
}

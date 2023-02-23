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
// Foobar is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Hearth. If not, see <https://www.gnu.org/licenses/>.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use clap::Parser;
use hearth_core::runtime::{RuntimeBuilder, RuntimeConfig};
use hearth_network::auth::ServerAuthenticator;
use hearth_rpc::*;
use hearth_types::*;
use remoc::robs::hash_map::{HashMapSubscription, ObservableHashMap};
use remoc::rtc::{async_trait, LocalRwLock, ServerSharedMut};
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

/// The constant peer ID for this peer (the server).
pub const SELF_PEER_ID: PeerId = PeerId(0);

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
    hearth_core::init_logging();

    let authenticator = ServerAuthenticator::from_password(args.password.as_bytes()).unwrap();
    let authenticator = Arc::new(authenticator);

    info!("Binding to {:?}", args.bind);
    let listener = match TcpListener::bind(args.bind).await {
        Ok(l) => l,
        Err(err) => {
            error!("Failed to listen: {:?}", err);
            return;
        }
    };

    let peer_info = PeerInfo { nickname: None };

    debug!("Creating peer provider");
    let peer_provider = PeerProviderImpl::new();
    let peer_provider = Arc::new(LocalRwLock::new(peer_provider));

    let (peer_provider_server, peer_provider_client) =
        PeerProviderServerSharedMut::<_, remoc::codec::Default>::new(peer_provider.clone(), 1024);

    debug!("Spawning peer provider server");
    tokio::spawn(async move {
        debug!("Running peer provider server");
        peer_provider_server.serve(true).await;
    });

    debug!("Initializing runtime");
    let config = RuntimeConfig {
        peer_provider: peer_provider_client.clone(),
        this_peer: SELF_PEER_ID,
        info: peer_info.clone(),
    };

    let mut builder = RuntimeBuilder::new();
    builder.add_plugin(hearth_cognito::WasmPlugin::new());

    let runtime = builder.run(config);
    let peer_api = runtime.clone().serve_peer_api();
    peer_provider
        .write()
        .await
        .add_peer(SELF_PEER_ID, peer_api, peer_info);

    debug!("Initializing IPC");
    let daemon_listener = match hearth_ipc::Listener::new().await {
        Ok(l) => l,
        Err(err) => {
            error!("IPC listener setup error: {:?}", err);
            return;
        }
    };

    let daemon_offer = DaemonOffer {
        peer_provider: peer_provider_client.to_owned(),
        peer_id: SELF_PEER_ID,
        process_factory: runtime.process_factory_client.clone(),
    };

    listen(listener, peer_provider, peer_provider_client, authenticator);
    hearth_ipc::listen(daemon_listener, daemon_offer);
    hearth_core::wait_for_interrupt().await;
    info!("Interrupt received; exiting server");
}

fn listen(
    listener: TcpListener,
    peer_provider: Arc<LocalRwLock<PeerProviderImpl>>,
    peer_provider_client: PeerProviderClient,
    authenticator: Arc<ServerAuthenticator>,
) {
    debug!("Spawning listen thread");
    tokio::spawn(async move {
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
            let peer_provider = peer_provider.clone();
            let peer_provider_client = peer_provider_client.to_owned();
            let authenticator = authenticator.clone();
            tokio::task::spawn(async move {
                on_accept(
                    peer_provider,
                    peer_provider_client,
                    authenticator,
                    socket,
                    addr,
                )
                .await;
            });
        }
    });
}

async fn on_accept(
    peer_provider: Arc<LocalRwLock<PeerProviderImpl>>,
    peer_provider_client: PeerProviderClient,
    authenticator: Arc<ServerAuthenticator>,
    mut client: TcpStream,
    addr: SocketAddr,
) {
    info!("Authenticating with client {:?}", addr);
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

    debug!("Initializing Remoc connection");
    use remoc::rch::base::{Receiver, Sender};
    let cfg = remoc::Cfg::default();
    let (conn, mut tx, mut rx): (_, Sender<ServerOffer>, Receiver<ClientOffer>) =
        match remoc::Connect::io(cfg, client_rx, client_tx).await {
            Ok(v) => v,
            Err(err) => {
                error!("Remoc connection failure: {:?}", err);
                return;
            }
        };

    debug!("Spawning Remoc connection thread");
    let join_connection = tokio::spawn(conn);

    debug!("Generating peer ID");
    let peer_id = peer_provider.write().await.get_next_peer();
    info!("Generated peer ID for new client: {:?}", peer_id);

    debug!("Sending server offer to client");
    tx.send(ServerOffer {
        peer_provider: peer_provider_client,
        new_id: peer_id,
    })
    .await
    .unwrap();

    debug!("Receiving client offer");
    let offer: ClientOffer = match rx.recv().await {
        Ok(Some(o)) => o,
        Ok(None) => {
            error!("Client hung up while waiting for offer");
            return;
        }
        Err(err) => {
            error!("Failed to receive client offer: {:?}", err);
            return;
        }
    };

    debug!("Getting peer {:?} info", peer_id);
    let peer_info = match offer.peer_api.get_info().await {
        Ok(i) => i,
        Err(err) => {
            error!("Failed to retrieve client peer info: {:?}", err);
            return;
        }
    };

    debug!("Adding peer {:?} to peer provider", peer_id);
    peer_provider
        .write()
        .await
        .add_peer(peer_id, offer.peer_api, peer_info);

    debug!("Waiting to join Remoc connection thread");
    match join_connection.await {
        Err(err) => {
            error!(
                "Tokio error while joining peer {:?} connection thread: {:?}",
                peer_id, err
            );
        }
        Ok(Err(remoc::chmux::ChMuxError::StreamClosed)) => {
            info!("Peer {:?} disconnected", peer_id);
        }
        Ok(Err(err)) => {
            error!(
                "Remoc chmux error while joining peer {:?} connection thread: {:?}",
                peer_id, err
            );
        }
        Ok(Ok(())) => {}
    }

    debug!("Removing peer from peer provider");
    peer_provider.write().await.remove_peer(peer_id);
}

struct PeerProviderImpl {
    next_peer: PeerId,
    peer_list: ObservableHashMap<PeerId, PeerInfo>,
    peer_apis: HashMap<PeerId, PeerApiClient>,
}

#[async_trait]
impl PeerProvider for PeerProviderImpl {
    async fn find_peer(&self, id: PeerId) -> ResourceResult<PeerApiClient> {
        self.peer_apis
            .get(&id)
            .cloned()
            .ok_or(ResourceError::Unavailable)
    }

    async fn follow_peer_list(&self) -> CallResult<HashMapSubscription<PeerId, PeerInfo>> {
        Ok(self.peer_list.subscribe(1024))
    }
}

impl PeerProviderImpl {
    pub fn new() -> Self {
        Self {
            next_peer: PeerId(1), // start from 1 to accomodate [SELF_PEER_ID]
            peer_list: Default::default(),
            peer_apis: Default::default(),
        }
    }

    pub fn get_next_peer(&mut self) -> PeerId {
        let id = self.next_peer;
        self.next_peer.0 += 1;
        id
    }

    pub fn add_peer(&mut self, id: PeerId, api: PeerApiClient, info: PeerInfo) {
        self.peer_list.insert(id, info);
        self.peer_apis.insert(id, api);
    }

    pub fn remove_peer(&mut self, id: PeerId) {
        self.peer_list.remove(&id);
        self.peer_apis.remove(&id);
    }
}

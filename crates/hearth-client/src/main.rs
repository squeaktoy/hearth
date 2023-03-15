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

use std::{
    net::{SocketAddr, ToSocketAddrs},
    str::FromStr, path::PathBuf,
};

use clap::Parser;
use hearth_core::runtime::{RuntimeBuilder, RuntimeConfig};
use hearth_network::auth::login;
use hearth_rpc::*;
use tokio::net::TcpStream;
use tracing::{debug, error, info};

/// Client program to the Hearth virtual space server.
#[derive(Parser, Debug)]
pub struct Args {
    /// IP address and port of the server to connect to.
    // TODO support DNS resolution too
    #[arg(short, long)]
    pub server: String,

    /// Password to use to authenticate to the server. Defaults to empty.
    #[arg(short, long, default_value = "")]
    pub password: String,

    /// A configuration file to use if not the default one.
    #[arg(short, long)]
    pub config: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    hearth_core::init_logging();
    let server = match SocketAddr::from_str(&args.server) {
        Err(_) => {
            info!(
                "Failed to parse \'{}\' to SocketAddr, attempting DNS resolution",
                args.server
            );
            match args.server.to_socket_addrs() {
                Err(err) => {
                    error!("Failed to resolve IP: {:?}", err);
                    return;
                }
                Ok(addrs) => match addrs.last() {
                    None => return,
                    Some(addr) => addr,
                },
            }
        }
        Ok(addr) => addr,
    };

    info!("Connecting to server at {:?}", server);
    let mut socket = match TcpStream::connect(server).await {
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
    let config = RuntimeConfig {
        peer_provider: offer.peer_provider.clone(),
        this_peer: offer.new_id,
        info: peer_info,
    };

    let config_path = args
        .config
        .unwrap_or_else(|| hearth_core::get_config_path());
    let config_file = hearth_core::load_config(&config_path).unwrap();
    let mut builder = RuntimeBuilder::new(config_file);
    builder.add_plugin(hearth_cognito::WasmPlugin::new());

    let runtime = builder.run(config);
    let peer_api = runtime.clone().serve_peer_api();

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
        peer_provider: offer.peer_provider,
        peer_id: offer.new_id,
        process_factory: runtime.process_factory_client.clone(),
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

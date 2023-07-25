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

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use hearth_core::runtime::{RuntimeBuilder, RuntimeConfig};
use hearth_network::auth::ServerAuthenticator;
use hearth_types::*;
use tokio::net::{TcpListener, TcpStream};
use tracing::{debug, error, info};

/// The constant peer ID for this peer (the server).
pub const SELF_PEER_ID: PeerId = PeerId(0);

/// The Hearth virtual space server program.
#[derive(Parser, Debug)]
pub struct Args {
    /// IP address and port to listen on.
    #[clap(short, long)]
    pub bind: Option<SocketAddr>,

    /// Password to use to authenticate with clients. Defaults to empty.
    #[clap(short, long, default_value = "")]
    pub password: String,

    /// A configuration file to use if not the default one.
    #[clap(short, long)]
    pub config: Option<PathBuf>,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    hearth_core::init_logging();

    let authenticator = ServerAuthenticator::from_password(args.password.as_bytes()).unwrap();
    let authenticator = Arc::new(authenticator);

    debug!("Initializing runtime");
    let config = RuntimeConfig {
        this_peer: SELF_PEER_ID,
    };

    let config_path = args
        .config
        .unwrap_or_else(|| hearth_core::get_config_path());
    let config_file = hearth_core::load_config(&config_path).unwrap();
    let mut builder = RuntimeBuilder::new(config_file);
    builder.add_plugin(hearth_cognito::WasmPlugin::new());

    let (_runtime, join_handles) = builder.run(config);

    debug!("Initializing IPC");
    let daemon_listener = match hearth_ipc::Listener::new().await {
        Ok(l) => l,
        Err(err) => {
            error!("IPC listener setup error: {:?}", err);
            return;
        }
    };

    info!("Binding to {:?}", args.bind);
    if let Some(bind) = args.bind {
        let listener = match TcpListener::bind(bind).await {
            Ok(l) => l,
            Err(err) => {
                error!("Failed to listen: {:?}", err);
                return;
            }
        };
        listen(listener, authenticator);
    } else {
        info!("Server running in headless mode");
    }

    daemon_listener.listen();
    hearth_core::wait_for_interrupt().await;

    info!("Interrupt received; exiting server");
    for join in join_handles {
        join.abort();
    }
}

fn listen(listener: TcpListener, authenticator: Arc<ServerAuthenticator>) {
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
            let authenticator = authenticator.clone();
            tokio::task::spawn(async move {
                on_accept(
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
    let _client_rx = AsyncDecryptor::new(&client_key, client_rx);
    let _client_tx = AsyncEncryptor::new(&server_key, client_tx);
}

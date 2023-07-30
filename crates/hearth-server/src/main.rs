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

use std::path::PathBuf;
use std::sync::Arc;
use std::{net::SocketAddr, ops::DerefMut};

use clap::Parser;
use hearth_core::process::Process;
use hearth_core::runtime::Runtime;
use hearth_core::{
    process::{
        context::{Capability, ContextMessage},
        factory::ProcessInfo,
        Connection, ProcessStore,
    },
    runtime::{RuntimeBuilder, RuntimeConfig},
};
use hearth_network::auth::ServerAuthenticator;
use hearth_types::{wasm::WasmSpawnInfo, *};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
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

    /// The init system to run.
    #[clap(short, long)]
    pub init: PathBuf,

    /// A path to the guest-side filesystem root.
    #[clap(short, long)]
    pub root: PathBuf,
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

    let config_path = args.config.unwrap_or_else(hearth_core::get_config_path);
    let config_file = hearth_core::load_config(&config_path).unwrap();

    let (network_root_tx, network_root_rx) = oneshot::channel();
    let mut init = hearth_init::InitPlugin::new();
    init.add_hook("hearth.init.Server".into(), network_root_tx);

    let mut builder = RuntimeBuilder::new(config_file);
    builder.add_plugin(hearth_cognito::WasmPlugin::new());
    builder.add_plugin(hearth_fs::FsPlugin::new(args.root));
    builder.add_plugin(init);
    let (runtime, join_handles) = builder.run(config).await;

    debug!("Loading init system module");
    let wasm_data = std::fs::read(args.init).unwrap();
    let wasm_lump = runtime.lump_store.add_lump(wasm_data.into()).await;

    debug!("Running init system");
    let mut parent = runtime.process_factory.spawn(ProcessInfo {}, Flags::SEND);
    let wasm_spawner = parent
        .get_service("hearth.cognito.WasmProcessSpawner")
        .expect("Wasm spawner service not found");

    let spawn_info = WasmSpawnInfo {
        lump: wasm_lump,
        entrypoint: None,
    };

    parent
        .send(
            wasm_spawner,
            ContextMessage {
                data: serde_json::to_vec(&spawn_info).unwrap(),
                caps: vec![0],
            },
        )
        .unwrap();

    debug!("Initializing IPC");
    let daemon_listener = match hearth_ipc::Listener::new().await {
        Ok(l) => l,
        Err(err) => {
            error!("IPC listener setup error: {:?}", err);
            return;
        }
    };

    if let Some(addr) = args.bind {
        tokio::spawn(async move {
            bind(network_root_rx, addr, runtime.clone(), authenticator).await;
        });
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

async fn bind(
    on_network_root: oneshot::Receiver<(Process, usize)>,
    addr: SocketAddr,
    runtime: Arc<Runtime>,
    authenticator: Arc<ServerAuthenticator>,
) {
    info!("Waiting for network root cap hook");
    let network_root = on_network_root.await.unwrap();
    let network_root = Arc::new(network_root);

    info!("Binding to {:?}", addr);
    let listener = match TcpListener::bind(addr).await {
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
        let store = runtime.process_store.clone();
        let network_root = network_root.clone();
        let authenticator = authenticator.clone();
        tokio::task::spawn(async move {
            on_accept(store, authenticator, socket, addr, move |conn| {
                let (ctx, handle) = network_root.as_ref();
                ctx.export_connection_root(*handle, conn).unwrap();
            })
            .await;
        });
    }
}

async fn on_accept(
    store: Arc<ProcessStore>,
    authenticator: Arc<ServerAuthenticator>,
    mut client: TcpStream,
    addr: SocketAddr,
    export_root: impl FnOnce(&mut Connection),
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
    let conn = hearth_network::connection::Connection::new(client_rx, client_tx);

    let (root_cap_tx, client_root) = tokio::sync::oneshot::channel();
    let on_root_cap = {
        let store = store.clone();
        move |root: Capability| {
            if let Err(dropped) = root_cap_tx.send(root) {
                dropped.free(store.as_ref());
            }
        }
    };

    info!("Beginning connection");
    let conn = Connection::new(
        store.clone(),
        conn.op_rx,
        conn.op_tx,
        Some(Box::new(on_root_cap)),
    );

    info!("Sending the client our root cap");
    export_root(conn.lock().deref_mut());

    info!("Waiting for client's root cap...");
    let client_root = match client_root.await {
        Ok(cap) => cap,
        Err(err) => {
            eprintln!("Client's root cap was never received: {:?}", err);
            return;
        }
    };

    client_root.free(store.as_ref());
}

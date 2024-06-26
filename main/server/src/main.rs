use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use clap::Parser;
use hearth_network::auth::ServerAuthenticator;
use hearth_runtime::connection::Connection;
use hearth_runtime::flue::{OwnedCapability, PostOffice};
use hearth_runtime::runtime::Runtime;
use hearth_runtime::runtime::{RuntimeBuilder, RuntimeConfig};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tracing::{debug, error, info};

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
    ///
    /// [default: <ROOT>/init.wasm]
    #[clap(short, long)]
    pub init: Option<PathBuf>,

    /// A path to the guest-side filesystem root.
    #[clap(short, long)]
    pub root: PathBuf,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();
    hearth_runtime::init_logging();

    let authenticator = ServerAuthenticator::from_password(args.password.as_bytes()).unwrap();
    let authenticator = Arc::new(authenticator);

    debug!("Initializing runtime");
    let config = RuntimeConfig {};

    let (network_root_tx, network_root_rx) = oneshot::channel();
    let init = args.init.unwrap_or(args.root.join("init.wasm"));
    let mut init = hearth_init::InitPlugin::new(init);
    init.add_hook("hearth.init.Server".into(), network_root_tx);

    let mut builder = RuntimeBuilder::new();
    builder.add_plugin(hearth_time::TimePlugin);
    builder.add_plugin(hearth_wasm::WasmPlugin::default());
    builder.add_plugin(hearth_fs::FsPlugin::new(args.root));
    builder.add_plugin(init);
    builder.add_plugin(hearth_daemon::DaemonPlugin::default());
    let runtime = builder.run(config).await;

    if let Some(addr) = args.bind {
        tokio::spawn(async move {
            bind(network_root_rx, addr, runtime.clone(), authenticator).await;
        });
    } else {
        info!("Server running in headless mode");
    }

    hearth_runtime::wait_for_interrupt().await;

    info!("Interrupt received; exiting server");
}

async fn bind(
    on_network_root: oneshot::Receiver<OwnedCapability>,
    addr: SocketAddr,
    runtime: Arc<Runtime>,
    authenticator: Arc<ServerAuthenticator>,
) {
    info!("Waiting for network root cap hook");
    let network_root = on_network_root.await.unwrap();

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
        let post = runtime.post.clone();
        let authenticator = authenticator.clone();
        let network_root = network_root.clone();
        tokio::task::spawn(async move {
            on_accept(post, authenticator, socket, addr, network_root).await;
        });
    }
}

async fn on_accept(
    post: Arc<PostOffice>,
    authenticator: Arc<ServerAuthenticator>,
    mut client: TcpStream,
    addr: SocketAddr,
    network_root: OwnedCapability,
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

    info!("Beginning connection");
    let conn = Connection::begin(post, conn.op_rx, conn.op_tx, Some(root_cap_tx));

    info!("Sending the client our root cap");
    conn.export_root(network_root);

    info!("Waiting for client's root cap...");
    let _client_root = match client_root.await {
        Ok(cap) => cap,
        Err(err) => {
            eprintln!("Client's root cap was never received: {:?}", err);
            return;
        }
    };

    info!("Client sent a root cap!");
}

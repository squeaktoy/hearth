use std::{
    ops::{Deref, DerefMut},
    path::PathBuf,
    sync::Arc,
};

use hearth_init::InitPlugin;
use hearth_ipc::get_socket_path;
use hearth_runtime::{
    connection::Connection,
    flue::OwnedCapability,
    runtime::{Plugin, Runtime, RuntimeBuilder},
    tokio::{
        self,
        net::{UnixListener, UnixStream},
        sync::oneshot,
    },
};

pub struct Listener {
    pub uds: UnixListener,
    pub path: PathBuf,
}

impl Drop for Listener {
    fn drop(&mut self) {
        match std::fs::remove_file(&self.path) {
            Ok(_) => {}
            Err(e) => tracing::error!("Could not delete UnixListener {:?}", e),
        }
    }
}

impl Deref for Listener {
    type Target = UnixListener;

    fn deref(&self) -> &UnixListener {
        &self.uds
    }
}

impl DerefMut for Listener {
    fn deref_mut(&mut self) -> &mut UnixListener {
        &mut self.uds
    }
}

impl Listener {
    pub async fn new() -> std::io::Result<Self> {
        use std::io::{Error, ErrorKind};

        let sock_path = match get_socket_path() {
            Some(p) => p,
            None => {
                let kind = ErrorKind::NotFound;
                let msg = "Failed to find a socket path";
                tracing::error!(msg);
                return Err(Error::new(kind, msg));
            }
        };

        match UnixStream::connect(&sock_path).await {
            Ok(_) => {
                let kind = ErrorKind::AddrInUse;
                let error = Error::new(
                    kind,
                    "Socket is already in use. Another instance of Hearth may be running.",
                );
                return Err(error);
            }
            Err(ref err) if err.kind() == ErrorKind::ConnectionRefused => {
                tracing::warn!("Found leftover socket; removing.");
                std::fs::remove_file(&sock_path)?;
            }
            Err(ref err) if err.kind() == ErrorKind::NotFound => {}
            Err(err) => return Err(err),
        }

        tracing::info!("Making socket at: {:?}", sock_path);
        let uds = UnixListener::bind(&sock_path)?;
        let path = sock_path.to_path_buf();
        Ok(Self { uds, path })
    }

    pub async fn accept_next(&self) -> hearth_ipc::Connection {
        let stream = loop {
            match self.accept().await {
                Ok((socket, addr)) => {
                    tracing::debug!("Accepting IPC connection from {:?}", addr);
                    break socket;
                }
                Err(err) => {
                    tracing::error!("IPC listen error: {:?}", err);
                }
            }
        };

        let (rx, tx) = stream.into_split();
        hearth_ipc::Connection::new(rx, tx)
    }
}

#[derive(Default)]
pub struct DaemonPlugin {}

impl Plugin for DaemonPlugin {
    fn finalize(mut self, builder: &mut RuntimeBuilder) {
        let init = builder
            .get_plugin_mut::<InitPlugin>()
            .expect("InitPlugin not found");

        let (root_tx, root_rx) = oneshot::channel();
        init.add_hook("hearth.init.Daemon".into(), root_tx);

        builder.add_runner(move |runtime| {
            tokio::spawn(async move {
                tracing::info!("Waiting for IPC daemon hook...");

                let root_cap = match root_rx.await {
                    Ok(root) => root,
                    Err(err) => {
                        tracing::warn!("error while waiting for daemon root cap: {}", err);
                        return;
                    }
                };

                tracing::info!("Listening on IPC daemon...");

                let listener = match Listener::new().await {
                    Ok(l) => l,
                    Err(err) => {
                        tracing::warn!("error while listening on IPC daemon: {}", err);
                        return;
                    }
                };

                loop {
                    let transport = listener.accept_next().await;
                    self.on_accept(root_cap.clone(), &runtime, transport);
                }
            });
        });
    }
}

impl DaemonPlugin {
    /// Performs a connection handshake with an IPC client and adds the new
    /// connection to the runtime.
    pub fn on_accept(
        &mut self,
        root_cap: OwnedCapability,
        runtime: &Arc<Runtime>,
        transport: hearth_ipc::Connection,
    ) {
        tracing::info!("Beginning IPC connection");
        let conn = Connection::begin(runtime.post.clone(), transport.op_rx, transport.op_tx, None);

        tracing::info!("Sending the IPC client our root cap");
        conn.export_root(root_cap);
    }
}

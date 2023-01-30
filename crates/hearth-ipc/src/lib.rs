use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

use hearth_rpc::DaemonOffer;
use tokio::net::{UnixListener, UnixStream};

/// Returns the path of the Hearth IPC socket.
///
/// If the HEARTH_SOCK environment variable is set, then that is used for the
/// path. Otherwise, "$XDG_RUNTIME_DIR/hearth.sock" is used. If XDG_RUNTIME_DIR
/// is not set, then this function returns `None`.
pub fn get_socket_path() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("HEARTH_SOCK") {
        match path.clone().try_into() {
            Ok(path) => return Some(path),
            Err(err) => {
                tracing::error!("Failed to cast HEARTH_SOCK ({}) to path: {:?}", path, err);
            }
        }
    }

    if let Ok(path) = std::env::var("XDG_RUNTIME_DIR") {
        match TryInto::<PathBuf>::try_into(path.clone()) {
            Ok(path) => {
                let path = path.join("hearth.sock");
                return Some(path);
            }
            Err(err) => {
                tracing::error!(
                    "Failed to cast XDG_RUNTIME_DIR ({}) to path: {:?}",
                    path,
                    err
                );
            }
        }
    }

    None
}

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
                tracing::warn!(
                    "Socket is already in use. Another instance of Hearth may be running."
                );
                let kind = ErrorKind::AddrInUse;
                let error = Error::new(kind, "Socket is already in use.");
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
}

/// Spawns a Tokio thread to respond to connections on the domain socket.
pub fn listen(listener: Listener, offer: DaemonOffer) {
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    tracing::debug!("Accepting IPC connection from {:?}", addr);
                    let offer = offer.clone();
                    tokio::spawn(async move {
                        on_accept(socket, offer).await;
                    });
                }
                Err(err) => {
                    tracing::error!("IPC listen error: {:?}", err);
                }
            }
        }
    });
}

async fn on_accept(socket: UnixStream, offer: DaemonOffer) {
    let (sock_rx, sock_tx) = tokio::io::split(socket);

    use hearth_rpc::remoc::{
        rch::base::{Receiver, Sender},
        Cfg, Connect,
    };

    let cfg = Cfg::default();
    let (conn, mut tx, _rx): (_, Sender<DaemonOffer>, Receiver<()>) =
        match Connect::io(cfg, sock_rx, sock_tx).await {
            Ok(v) => v,
            Err(err) => {
                tracing::error!("Remoc connection failure: {:?}", err);
                return;
            }
        };

    tokio::spawn(conn);

    tx.send(offer).await.unwrap();
}

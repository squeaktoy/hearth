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

use std::ops::{Deref, DerefMut};
use std::path::PathBuf;

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

    /// Spawns a Tokio thread to respond to connections on the domain socket.
    pub fn listen(self) {
        tokio::spawn(async move {
            loop {
                match self.accept().await {
                    Ok((_socket, addr)) => {
                        tracing::debug!("Accepting IPC connection from {:?}", addr);
                    }
                    Err(err) => {
                        tracing::error!("IPC listen error: {:?}", err);
                    }
                }
            }
        });
    }
}

/// Connects to the Hearth daemon and returns a [UnixStream].
pub async fn connect() -> std::io::Result<UnixStream> {
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

    UnixStream::connect(&sock_path).await
}

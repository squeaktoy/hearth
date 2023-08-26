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

use tokio::net::UnixStream;

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

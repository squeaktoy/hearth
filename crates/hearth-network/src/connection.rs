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

use hearth_types::protocol::CapOperation;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    sync::mpsc::{unbounded_channel, UnboundedReceiver, UnboundedSender},
};

pub struct Connection {
    /// An outgoing channel for capability operations.
    pub op_tx: UnboundedSender<CapOperation>,

    /// A channel for incoming capability operations.
    pub op_rx: UnboundedReceiver<CapOperation>,
}

impl Connection {
    /// Creates a connection for the given transport.
    pub fn new(
        mut rx: impl AsyncRead + Unpin + Send + 'static,
        mut tx: impl AsyncWrite + Unpin + Send + 'static,
    ) -> Self {
        let (outgoing_tx, mut outgoing_rx) = unbounded_channel();
        let (incoming_tx, incoming_rx) = unbounded_channel();

        tokio::spawn(async move {
            while let Some(op) = outgoing_rx.recv().await {
                let payload = bincode::serialize(&op).unwrap();
                let len = payload.len() as u32;
                tx.write_u32_le(len).await.unwrap();
                tx.write_all(&payload).await.unwrap();
            }
        });

        #[allow(clippy::read_zero_byte_vec)]
        tokio::spawn(async move {
            let mut buf = Vec::new();
            loop {
                let len = rx.read_u32_le().await.unwrap();
                buf.resize(len as usize, 0);
                rx.read_exact(&mut buf).await.unwrap();
                let op = bincode::deserialize(&buf).unwrap();
                if incoming_tx.send(op).is_err() {
                    break;
                }
            }
        });

        Self {
            op_tx: outgoing_tx,
            op_rx: incoming_rx,
        }
    }
}

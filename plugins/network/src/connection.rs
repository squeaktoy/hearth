use flume::{unbounded, Receiver, Sender};
use hearth_schema::protocol::CapOperation;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub struct Connection {
    /// An outgoing channel for capability operations.
    pub op_tx: Sender<CapOperation>,

    /// A channel for incoming capability operations.
    pub op_rx: Receiver<CapOperation>,
}

impl Connection {
    /// Creates a connection for the given transport.
    pub fn new(
        mut rx: impl AsyncRead + Unpin + Send + 'static,
        mut tx: impl AsyncWrite + Unpin + Send + 'static,
    ) -> Self {
        let (outgoing_tx, outgoing_rx) = unbounded();
        let (incoming_tx, incoming_rx) = unbounded();

        tokio::spawn(async move {
            while let Ok(op) = outgoing_rx.recv_async().await {
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

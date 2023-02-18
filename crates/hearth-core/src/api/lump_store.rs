use std::collections::HashMap;

use bytes::Buf;
use hearth_rpc::remoc::{self, rtc::CallError};
use remoc::robj::lazy_blob::{LazyBlob, Provider as BlobProvider};
use tokio::sync::RwLock;

use super::*;

struct Lump {
    provider: BlobProvider,
    blob: LazyBlob,
}

pub struct LumpStoreImpl {
    store: RwLock<HashMap<LumpId, Lump>>,
}

#[async_trait]
impl LumpStore for LumpStoreImpl {
    async fn upload_lump(&self, id: Option<LumpId>, data: LazyBlob) -> ResourceResult<LumpId> {
        if let Some(id) = id {
            if self.store.read().await.contains_key(&id) {
                return Ok(id);
            }
        }

        let data = match data.get().await {
            Ok(data) => data,
            Err(err) => {
                eprintln!("Downloading lump failed: {:?}", err);
                return Err(ResourceError::Unavailable);
            }
        };

        let checked_id = LumpId(
            blake3::Hasher::new()
                .update(data.chunk())
                .finalize()
                .as_bytes()
                .to_owned(),
        );

        if let Some(expected_id) = id {
            if expected_id != checked_id {
                error!(
                    "Lump hash mismatch (expected {}, got {})",
                    expected_id, checked_id
                );

                return Err(ResourceError::BadParams);
            }
        }

        let (blob, provider) = LazyBlob::provided(data.into());
        let lump = Lump { provider, blob };
        let mut store = self.store.write().await;
        store.insert(checked_id, lump);
        Ok(checked_id)
    }

    async fn download_lump(&self, id: LumpId) -> ResourceResult<LazyBlob> {
        self.store
            .read()
            .await
            .get(&id)
            .ok_or(ResourceError::Unavailable)
            .map(|l| l.blob.to_owned())
    }
}

impl LumpStoreImpl {
    pub fn new() -> Self {
        Self {
            store: Default::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id(bytes: &[u8]) -> LumpId {
        LumpId(
            blake3::Hasher::new()
                .update(bytes)
                .finalize()
                .as_bytes()
                .to_owned(),
        )
    }

    /// LazyBlobs don't work locally, so this needs to be pulled off in order
    /// to get them to work in these tests.
    async fn spawn_store() -> LumpStoreClient {
        use remoc::rch::base::{Receiver, Sender};

        let (a, b) = tokio::io::duplex(4096);
        let (a_rx, a_tx) = tokio::io::split(a);
        let (b_rx, b_tx) = tokio::io::split(b);

        let join_a = tokio::spawn(async move {
            let (conn, tx, _rx): (_, Sender<LumpStoreClient>, Receiver<()>) =
                remoc::Connect::io(Default::default(), a_rx, a_tx)
                    .await
                    .unwrap();
            tokio::task::spawn(conn);
            tx
        });

        let join_b = tokio::spawn(async move {
            let (conn, _tx, rx): (_, Sender<()>, Receiver<LumpStoreClient>) =
                remoc::Connect::io(Default::default(), b_rx, b_tx)
                    .await
                    .unwrap();
            tokio::task::spawn(conn);
            rx
        });

        let store = LumpStoreImpl::new();
        let store = Arc::new(store);
        let (store_server, store) = LumpStoreServerShared::new(store, 1024);

        tokio::spawn(async move {
            store_server.serve(true).await;
        });

        let mut tx = join_a.await.unwrap();
        let mut rx = join_b.await.unwrap();

        tx.send(store).await.unwrap();
        rx.recv().await.unwrap().unwrap()
    }

    #[tokio::test]
    async fn create_store() {
        let _store = spawn_store();
    }

    #[tokio::test]
    async fn upload_then_download() {
        const DATA: &[u8] = b"Hello, world!";
        let id = make_id(DATA);
        let data_blob = LazyBlob::new(DATA.into());
        let store = spawn_store().await;

        let uploaded = store
            .upload_lump(Some(id), data_blob)
            .await
            .expect("Failed to upload");

        assert_eq!(uploaded, id);

        let downloaded = store
            .download_lump(id)
            .await
            .expect("Failed to download")
            .get()
            .await
            .unwrap();

        assert_eq!(downloaded.chunk(), DATA);
    }

    #[tokio::test]
    async fn wrong_id() {
        const DATA: &[u8] = b"Hello, world!";
        let wrong = make_id(b"Wrong data!");
        let data_blob = LazyBlob::new(DATA.into());
        let store = spawn_store().await;
        let result = store.upload_lump(Some(wrong), data_blob).await;
        match result {
            Err(ResourceError::BadParams) => {}
            result => panic!("Unexpected result: {:?}", result),
        }
    }
}

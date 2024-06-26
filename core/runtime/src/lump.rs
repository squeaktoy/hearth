use std::collections::HashMap;

use bytes::{Buf, Bytes};
use hearth_schema::*;
use tokio::sync::RwLock;
use tracing::debug;

pub use bytes;

#[derive(Debug)]
struct Lump {
    data: Bytes,
}

#[derive(Debug, Default)]
pub struct LumpStoreImpl {
    store: RwLock<HashMap<LumpId, Lump>>,
}

impl LumpStoreImpl {
    pub fn new() -> Self {
        Self {
            store: Default::default(),
        }
    }

    pub async fn add_lump(&self, data: Bytes) -> LumpId {
        let id = LumpId(
            blake3::Hasher::new()
                .update(data.chunk())
                .finalize()
                .as_bytes()
                .to_owned(),
        );

        let mut store = self.store.write().await;
        store.entry(id).or_insert_with(|| {
            debug!("Storing lump {}", id);
            Lump { data }
        });

        id
    }

    pub async fn get_lump(&self, id: &LumpId) -> Option<Bytes> {
        self.store
            .read()
            .await
            .get(id)
            .map(|lump| lump.data.clone())
    }
}

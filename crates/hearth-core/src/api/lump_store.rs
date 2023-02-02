use hearth_rpc::remoc::robj::lazy_blob::LazyBlob;

use super::*;

pub struct LumpStoreImpl {}

#[async_trait]
impl LumpStore for LumpStoreImpl {
    async fn upload_lump(&self, id: Option<LumpId>, data: LazyBlob) -> ResourceResult<LumpId> {
        Ok(LumpId([0; 32]))
    }

    async fn download_lump(&self, id: LumpId) -> ResourceResult<LazyBlob> {
        Err(ResourceError::Unavailable)
    }
}

impl LumpStoreImpl {
    pub fn new() -> Self {
        Self {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use bytes::Buf;

    fn make_id(bytes: &[u8]) -> LumpId {
        LumpId(
            blake3::Hasher::new()
                .update(bytes)
                .finalize()
                .as_bytes()
                .to_owned(),
        )
    }

    #[test]
    fn create_store() {
        let _store = LumpStoreImpl::new();
    }

    #[tokio::test]
    async fn upload_then_download() {
        const DATA: &[u8] = b"Hello, world!";
        let id = make_id(DATA);
        let data_blob = LazyBlob::new(DATA.into());
        let store = LumpStoreImpl::new();

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
        let store = LumpStoreImpl::new();
        let result = store.upload_lump(Some(wrong), data_blob).await;
        match result {
            Err(ResourceError::BadParams) => {}
            result => panic!("Unexpected result: {:?}", result),
        }
    }
}

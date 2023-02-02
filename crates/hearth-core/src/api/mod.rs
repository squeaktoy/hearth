use hearth_rpc::*;
use hearth_types::*;
use remoc::rtc::async_trait;
use tracing::{debug, error, info};

pub mod lump_store;

/// The canonical [PeerApi] implementation, with full functionality.
pub struct PeerApiImpl {
    pub info: PeerInfo,
    pub lump_store: LumpStoreClient,
}

#[async_trait]
impl PeerApi for PeerApiImpl {
    async fn get_info(&self) -> CallResult<PeerInfo> {
        Ok(self.info.clone())
    }

    async fn get_process_store(&self) -> CallResult<ProcessStoreClient> {
        error!("Process stores are unimplemented");
        Err(remoc::rtc::CallError::RemoteForward)
    }

    async fn get_lump_store(&self) -> CallResult<LumpStoreClient> {
        Ok(self.lump_store.to_owned())
    }
}

use std::sync::Arc;

use hearth_rpc::{remoc::rtc::ServerShared, *};
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

/// Helper function to create and spawn a [PeerApiImpl] and all of its
/// subsystems.
pub fn spawn_peer_api(info: PeerInfo) -> PeerApiClient {
    debug!("Creating lump store");
    let lump_store = lump_store::LumpStoreImpl::new();
    let lump_store = Arc::new(lump_store);
    let (lump_store_server, lump_store) = LumpStoreServerShared::new(lump_store, 1024);

    debug!("Spawning lump store server thread");
    tokio::spawn(async move {
        lump_store_server.serve(true).await;
    });

    debug!("Creating peer API");
    let peer_api = PeerApiImpl { info, lump_store };
    let peer_api = Arc::new(peer_api);
    let (peer_api_server, peer_api) = PeerApiServerShared::new(peer_api, 1024);

    debug!("Spawning peer API server thread");
    tokio::spawn(async move {
        peer_api_server.serve(true).await;
    });

    peer_api
}

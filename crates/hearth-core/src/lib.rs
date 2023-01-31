use hearth_rpc::*;
use remoc::rtc::async_trait;
use tracing::error;

/// Helper function to set up console logging with reasonable defaults.
pub fn init_logging() {
    let format = tracing_subscriber::fmt::format().compact();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .event_format(format)
        .init();
}

/// The canonical [PeerApi] implementation, with full functionality.
pub struct PeerApiImpl {
    pub info: PeerInfo,
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
}

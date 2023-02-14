use std::collections::HashMap;
use std::sync::Arc;

use hearth_rpc::remoc::robs::hash_map::HashMapSubscription;
use hearth_rpc::*;
use hearth_types::*;
use remoc::robs::hash_map::ObservableHashMap;
use remoc::rtc::async_trait;
use tokio::sync::RwLock;
use tracing::{info, debug};

use crate::runtime::Runtime;

#[async_trait]
pub trait Process: Send + Sync + 'static {
    async fn on_message(&self, from: ProcessId, data: Vec<u8>);
}

pub struct ProcessContext {
    /// The ID of this process.
    pub this_pid: ProcessId,

    /// The runtime that this process is a part of.
    pub runtime: Arc<Runtime>,
}

pub struct ProcessWrapper {
    pub process: Box<dyn Process>,
    pub pid: ProcessId,
}

pub struct ProcessStoreInner {
    pub services: ObservableHashMap<String, LocalProcessId>,
    pub processes: HashMap<LocalProcessId, ProcessWrapper>,
    pub process_infos: ObservableHashMap<LocalProcessId, ProcessInfo>,
}

impl ProcessStoreInner {
    pub fn new() -> Self {
        Self {
            services: Default::default(),
            processes: Default::default(),
            process_infos: Default::default(),
        }
    }
}

pub struct ProcessStoreImpl(pub RwLock<ProcessStoreInner>);

impl ProcessStoreImpl {
    pub fn new() -> Self {
        Self(RwLock::new(ProcessStoreInner::new()))
    }
}

#[async_trait]
impl ProcessStore for ProcessStoreImpl {
    async fn print_hello_world(&self) -> CallResult<()> {
        info!("Hello, world!");
        Ok(())
    }

    async fn find_process(&self, pid: LocalProcessId) -> ResourceResult<ProcessApiClient> {
        Err(ResourceError::Unavailable)
    }

    async fn register_service(&self, pid: LocalProcessId, name: String) -> ResourceResult<()> {
        debug!("Registering service '{}' to {:?}", name, pid);

        let mut store = self.0.write().await;
        if !store.processes.contains_key(&pid) {
            debug!("Invalid local process ID");
            Err(ResourceError::Unavailable)
        } else if store.services.contains_key(&name) {
            debug!("Service name is taken");
            Err(ResourceError::BadParams)
        } else {
            store.services.insert(name, pid);
            Ok(())
        }
    }

    async fn deregister_service(&self, name: String) -> ResourceResult<()> {
        debug!("Deregistering service '{}'", name);
        if let None = self.0.write().await.services.remove(&name) {
            Err(ResourceError::Unavailable)
        } else {
            Ok(())
        }
    }

    async fn follow_process_list(
        &self,
    ) -> CallResult<HashMapSubscription<LocalProcessId, ProcessInfo>> {
        Ok(self.0.read().await.process_infos.subscribe(1024))
    }

    async fn follow_service_list(&self) -> CallResult<HashMapSubscription<String, LocalProcessId>> {
        Ok(self.0.read().await.services.subscribe(1024))
    }
}

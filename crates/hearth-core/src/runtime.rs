use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use hearth_rpc::remoc::rtc::ServerShared;
use hearth_rpc::*;
use hearth_types::PeerId;
use remoc::rtc::async_trait;
use tracing::{debug, error};

use crate::api::lump_store::LumpStoreImpl;
use crate::asset::{AssetLoader, AssetStore};
use crate::process::ProcessStoreImpl;

pub trait Plugin: 'static {
    fn build(&mut self, builder: &mut RuntimeBuilder);
}

pub struct RuntimeBuilder {
    plugins: HashMap<TypeId, Box<dyn Any>>,
    runners: Vec<Box<dyn FnOnce(Arc<Runtime>)>>,
    asset_store: AssetStore,
}

impl RuntimeBuilder {
    pub fn new() -> Self {
        Self {
            plugins: Default::default(),
            runners: Default::default(),
            asset_store: Default::default(),
        }
    }

    pub fn add_plugin<T: Plugin>(&mut self, mut plugin: T) {
        let id = plugin.type_id();
        debug!("Adding {:?} plugin", id);

        if self.plugins.contains_key(&id) {
            error!("Attempted to add plugin twice: {:?}", id);
            return;
        }

        plugin.build(self);
        self.plugins.insert(id, Box::new(plugin));
    }

    pub fn add_runner<F, R>(&mut self, cb: F)
    where
        F: FnOnce(Arc<Runtime>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        self.runners.push(Box::new(|runner| {
            tokio::spawn(async move {
                cb(runner).await;
            });
        }));
    }

    pub fn add_asset_loader(&mut self, class: String, loader: impl AssetLoader) {
        self.asset_store.add_loader(class, loader);
    }

    pub fn get_plugin<T: Plugin>(&self) -> Option<&T> {
        self.plugins
            .get(&TypeId::of::<T>())
            .map(|p| p.downcast_ref())
            .flatten()
    }

    pub fn run(self, config: RuntimeConfig) -> Arc<Runtime> {
        debug!("Spawning lump store server");
        let lump_store = Arc::new(LumpStoreImpl::new());
        let (lump_store_server, lump_store_client) = LumpStoreServerShared::new(lump_store, 1024);
        tokio::spawn(async move {
            lump_store_server.serve(true);
        });

        debug!("Spawning process store server");
        let process_store = Arc::new(ProcessStoreImpl::new());
        let (process_store_server, process_store_client) =
            ProcessStoreServerShared::new(process_store, 1024);
        tokio::spawn(async move {
            process_store_server.serve(true);
        });

        let runtime = Arc::new(Runtime {
            asset_store: self.asset_store,
            lump_store,
            lump_store_client,
            process_store,
            process_store_client,
            config,
        });

        for runner in self.runners {
            runner(runtime.clone());
        }

        runtime
    }
}

/// Configuration info for a runtime.
pub struct RuntimeConfig {
    /// The provider to other peers on the network.
    pub peer_provider: PeerProviderClient,

    /// The ID of this peer.
    pub this_peer: PeerId,

    /// The [PeerInfo] for this peer.
    pub info: PeerInfo,
}

pub struct Runtime {
    /// The configuration of this runtime.
    pub config: RuntimeConfig,

    //// The assets in this runtime.
    pub asset_store: AssetStore,

    /// This runtime's lump store.
    pub lump_store: Arc<LumpStoreImpl>,

    /// A clone-able client to this runtime's lump store.
    pub lump_store_client: LumpStoreClient,

    /// This runtime's process store.
    pub process_store: Arc<ProcessStoreImpl>,

    /// A clone-able client to this runtime's process store.
    pub process_store_client: ProcessStoreClient,
}

#[async_trait]
impl PeerApi for Runtime {
    async fn get_info(&self) -> CallResult<PeerInfo> {
        Ok(self.config.info.clone())
    }

    async fn get_process_store(&self) -> CallResult<ProcessStoreClient> {
        Ok(self.process_store_client.clone())
    }

    async fn get_lump_store(&self) -> CallResult<LumpStoreClient> {
        Ok(self.lump_store_client.clone())
    }
}

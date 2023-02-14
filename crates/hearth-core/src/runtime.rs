use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use hearth_rpc::{hearth_types::*, PeerProviderClient};
use tracing::{debug, error};

use crate::asset::{AssetLoader, AssetStore};

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
        let runtime = Arc::new(Runtime {
            peer_provider: config.peer_provider,
            this_peer: config.this_peer,
            asset_store: self.asset_store,
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
}

pub struct Runtime {
    //// The assets in this runtime.
    pub asset_store: AssetStore,

    /// The provider to other peers on the network.
    pub peer_provider: PeerProviderClient,

    /// The [PeerId] that this runtime represents.
    pub this_peer: PeerId,
}

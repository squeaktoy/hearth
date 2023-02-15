use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use hearth_rpc::remoc::rtc::ServerShared;
use hearth_rpc::*;
use hearth_types::PeerId;
use remoc::rtc::async_trait;
use tracing::{debug, error, warn};

use crate::asset::{AssetLoader, AssetStore};
use crate::lump::LumpStoreImpl;
use crate::process::ProcessStoreImpl;

/// Interface trait for plugins to the Hearth runtime.
///
/// Each plugin first builds onto a runtime using its `build` function and an
/// in-progress [RuntimeBuilder]. After all plugins are added, the runtime
/// starts, and the `run` method is called with a handle to the new runtime.
#[async_trait]
pub trait Plugin: 'static {
    /// Builds a runtime using this plugin. See [RuntimeBuilder] for more info.
    fn build(&mut self, builder: &mut RuntimeBuilder);

    /// Runs this plugin using an instantiated [Runtime].
    async fn run(&mut self, runtime: Arc<Runtime>);
}

struct PluginWrapper {
    plugin: Box<dyn Any>,
    runner: Box<dyn FnOnce(Box<dyn Any>, Arc<Runtime>)>,
}

/// Builder struct for a single Hearth [Runtime].
pub struct RuntimeBuilder {
    plugins: HashMap<TypeId, PluginWrapper>,
    runners: Vec<Box<dyn FnOnce(Arc<Runtime>)>>,
    asset_store: AssetStore,
}

impl RuntimeBuilder {
    /// Creates a new [RuntimeBuilder] with nothing loaded.
    pub fn new() -> Self {
        Self {
            plugins: Default::default(),
            runners: Default::default(),
            asset_store: Default::default(),
        }
    }

    /// Adds a plugin to the runtime.
    ///
    /// Plugins may use their [Plugin::build] method to add other plugins,
    /// asset loaders, runners, or anything else.
    pub fn add_plugin<T: Plugin>(&mut self, mut plugin: T) -> &mut Self {
        let id = plugin.type_id();
        debug!("Adding {:?} plugin", id);

        if self.plugins.contains_key(&id) {
            warn!("Attempted to add plugin twice: {:?}", id);
            return self;
        }

        plugin.build(self);

        self.plugins.insert(
            id,
            PluginWrapper {
                plugin: Box::new(plugin),
                runner: Box::new(|mut plugin, runtime| {
                    tokio::spawn(async move {
                        let mut plugin = plugin.downcast_ref_mut();
                        plugin.run(runtime);
                    });
                }),
            },
        );

        self
    }

    /// Adds a runner to the runtime.
    ///
    /// Runners are simple async functions that are spawned when the runtime is
    /// started and are passed a handle to the new runtime. This may be used
    /// for long-running event processing code or other functionality that
    /// lasts the runtime's lifetime.
    pub fn add_runner<F, R>(&mut self, cb: F) -> &mut Self
    where
        F: FnOnce(Arc<Runtime>) -> R + Send + Sync + 'static,
        R: Future<Output = ()> + Send,
    {
        self.runners.push(Box::new(|runner| {
            tokio::spawn(async move {
                cb(runner).await;
            });
        }));

        self
    }

    /// Adds a new asset loader for a given asset class.
    ///
    /// Logs an error event if the asset class already has a loader.
    pub fn add_asset_loader(&mut self, class: String, loader: impl AssetLoader) -> &mut Self {
        self.asset_store.add_loader(class, loader);
        self
    }

    /// Retrieves a reference to a plugin that has already been added.
    ///
    /// This function is intended to be used for dependencies of plugins, where
    /// a plugin may need to look up or modify the contents of a previously-
    /// added plugin. Using this function saves the code building the runtime
    /// the trouble of manually passing runtimes to other runtimes as
    /// dependencies.
    pub fn get_plugin<T: Plugin>(&self) -> Option<&T> {
        self.plugins
            .get(&TypeId::of::<T>())
            .map(|p| p.downcast_ref())
            .flatten()
    }

    /// Retrieves a mutable reference to a plugin that has already been added.
    ///
    /// Mutable version of [Self::get_plugin].
    pub fn get_plugin_mut<T: Plugin>(&mut self) -> Option<&mut T> {
        self.plugins
            .get_mut(&TypeId::of::<T>())
            .map(|p| p.downcast_ref_mut())
            .flatten()
    }

    /// Consumes this builder and starts up the full [Runtime].
    pub fn run(self, config: RuntimeConfig) -> Arc<Runtime> {
        debug!("Spawning lump store server");
        let lump_store = Arc::new(LumpStoreImpl::new());
        let (lump_store_server, lump_store_client) =
            LumpStoreServerShared::new(lump_store.clone(), 1024);
        tokio::spawn(async move {
            lump_store_server.serve(true).await;
        });

        debug!("Spawning process store server");
        let process_store = Arc::new(ProcessStoreImpl::new());
        let (process_store_server, process_store_client) =
            ProcessStoreServerShared::new(process_store.clone(), 1024);
        tokio::spawn(async move {
            process_store_server.serve(true).await;
        });

        let runtime = Arc::new(Runtime {
            asset_store: self.asset_store,
            lump_store,
            lump_store_client,
            process_store,
            process_store_client,
            config,
        });

        for (_id, wrapper) in self.plugins {
            let PluginWrapper { plugin, runner } = wrapper;
            runner(plugin, runtime.clone());
        }

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

/// An instance of a single Hearth runtime.
///
/// This contains all of the resources that are used by plugins, processes,
/// and network peers. A runtime can be built and started using
/// [RuntimeBuilder].
///
/// Note that Hearth uses Tokio for all of its asynchronous
/// task execution and IO, so it's assumed that a Tokio runtime has already
/// been created.
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

impl Runtime {
    /// Spawns a new [PeerApiServer] for this runtime and returns a client to it.
    pub fn serve_peer_api(self: Arc<Self>) -> PeerApiClient {
        debug!("Serving runtime PeerApi");
        let (server, client) = PeerApiServerShared::new(self, 1024);
        tokio::spawn(async move {
            server.serve(true).await;
        });

        client
    }
}

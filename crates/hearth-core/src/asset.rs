use std::collections::HashMap;
use std::sync::Arc;

use crate::lump::LumpStoreImpl;
use hearth_rpc::remoc::rtc::async_trait;
use sharded_slab::Slab;
use tracing::{debug, error};

#[async_trait]
pub trait AssetLoader: Send + Sync + 'static {
    type Asset: Send + Sync + 'static;

    async fn load_asset(&self, data: &[u8]) -> Self::Asset;
}

/// Object-safe wrapper trait for generic [AssetLoader]s.
#[async_trait]
pub trait AssetPool: Send + Sync + 'static {
    async fn load_asset(&self, data: &[u8]) -> usize;

    fn unload_asset(&self, id: usize);
}

/// Generic implementation of [AssetPool] for a given [AssetLoader].
///
/// Loads and stores assets loaded from a loader.
pub struct AssetPoolImpl<T: AssetLoader> {
    loader: T,
    assets: Slab<T::Asset>,
}

#[async_trait]
impl<T: AssetLoader> AssetPool for AssetPoolImpl<T> {
    async fn load_asset(&self, data: &[u8]) -> usize {
        let asset = self.loader.load_asset(data).await;
        let id = self.assets.insert(asset).unwrap();
        id
    }

    fn unload_asset(&self, id: usize) {
        self.assets.remove(id);
    }
}

impl<T: AssetLoader> AssetPoolImpl<T> {
    pub fn new(loader: T) -> Self {
        Self {
            loader,
            assets: Slab::new(),
        }
    }
}

pub struct AssetStore {
    class_to_pool: HashMap<String, usize>,
    pools: Vec<Box<dyn AssetPool>>,
    lump_store: Arc<LumpStoreImpl>,
}

impl AssetStore {
    pub fn new(lump_store: Arc<LumpStoreImpl>) -> Self {
        Self {
            class_to_pool: HashMap::new(),
            pools: Vec::new(),
            lump_store,
        }
    }

    pub fn add_loader(&mut self, class: String, loader: impl AssetLoader) {
        debug!("Adding asset loader {}", class);

        if self.class_to_pool.contains_key(&class) {
            error!("Asset loader for class {} has already been added!", class);
            return;
        }

        let id = self.pools.len();
        let pool = AssetPoolImpl::new(loader);
        self.pools.push(Box::new(pool));
        self.class_to_pool.insert(class, id);
    }

    pub fn has_loader(&self, class: &str) -> bool {
        self.class_to_pool.contains_key(class)
    }

    pub async fn load_asset(&self, class: &str, data: &[u8]) -> Handle {
        // TODO error reporting with eyre
        let pool_id = *self.class_to_pool.get(class).unwrap();
        let pool = self.pools.get(pool_id).unwrap();
        let asset_id = pool.load_asset(data).await;

        Handle {
            count: Arc::new(()),
            pool_id,
            asset_id,
        }
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct Handle {
    count: Arc<()>,
    pool_id: usize,
    asset_id: usize,
}

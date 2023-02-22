// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::collections::HashMap;
use std::sync::Arc;

use crate::lump::LumpStoreImpl;
use anyhow::{anyhow, Result};
use hearth_rpc::{hearth_types, remoc};
use hearth_types::LumpId;
use remoc::rtc::async_trait;
use sharded_slab::Slab;
use tracing::{debug, error};

#[async_trait]
pub trait AssetLoader: Send + Sync + 'static {
    type Asset: Send + Sync + 'static;

    async fn load_asset(&self, data: &[u8]) -> Result<Self::Asset>;
}

/// Object-safe wrapper trait for generic [AssetLoader]s.
#[async_trait]
pub trait AssetPool: Send + Sync + 'static {
    async fn load_asset(&self, data: &[u8]) -> Result<usize>;

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
    async fn load_asset(&self, data: &[u8]) -> Result<usize> {
        let asset = self.loader.load_asset(data).await?;
        let id = self.assets.insert(asset).unwrap();
        Ok(id)
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

    pub fn add_loader<T: AssetLoader>(&mut self, class: String, loader: T) {
        let type_name = std::any::type_name::<T>();
        debug!("Adding asset loader {} for class '{}'", type_name, class);

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

    pub async fn load_asset(&self, class: &str, lump: &LumpId) -> Result<Handle> {
        let pool_id = *self
            .class_to_pool
            .get(class)
            .ok_or_else(|| anyhow!("Could not find asset loader for class '{}'", class))?;
        let pool = self.pools.get(pool_id).unwrap(); // this should never panic; if it does it's a bug
        let data = self
            .lump_store
            .get_lump(lump)
            .await
            .ok_or_else(|| anyhow!("Failed to get lump {}", lump))?;
        let asset_id = pool.load_asset(&data).await?;

        Ok(Handle {
            count: Arc::new(()),
            pool_id,
            asset_id,
        })
    }
}

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct Handle {
    count: Arc<()>,
    pool_id: usize,
    asset_id: usize,
}

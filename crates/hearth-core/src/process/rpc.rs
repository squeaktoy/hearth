// Copyright (c) 2023 the Hearth contributors.
// SPDX-License-Identifier: AGPL-3.0-or-later
//
// This file is part of Hearth.
//
// Hearth is free software: you can redistribute it and/or modify it under the
// terms of the GNU Affero General Public License as published by the Free
// Software Foundation, either version 3 of the License, or (at your option)
// any later version.
//
// Hearth is distributed in the hope that it will be useful, but WITHOUT ANY
// WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS
// FOR A PARTICULAR PURPOSE. See the GNU Affero General Public License for more
// details.
//
// You should have received a copy of the GNU Affero General Public License
// along with Hearth. If not, see <https://www.gnu.org/licenses/>.

use std::sync::Arc;

use hearth_rpc::hearth_types::LocalProcessId;
use hearth_rpc::remoc::robs::list::ListSubscription;
use hearth_rpc::remoc::rtc::ServerShared;
use hearth_rpc::*;
use remoc::robs::hash_map::HashMapSubscription;
use remoc::rtc::async_trait;
use tracing::info;

use super::factory::{ProcessFactory, ProcessWrapper};
use super::local::LocalProcess;
use super::registry::Registry;
use super::store::ProcessStoreTrait;

pub struct ProcessStoreImpl<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    factory: Arc<ProcessFactory<Store>>,
    registry: Arc<Registry<Store>>,
}

#[async_trait]
impl<Store> hearth_rpc::ProcessStore for ProcessStoreImpl<Store>
where
    Store: ProcessStoreTrait + Send + Sync + 'static,
    Store::Entry: From<LocalProcess>,
{
    async fn print_hello_world(&self) -> CallResult<()> {
        info!("Hello, world!");
        Ok(())
    }

    async fn find_process(&self, pid: LocalProcessId) -> ResourceResult<ProcessApiClient> {
        let wrapper = self
            .factory
            .get_pid_wrapper(pid)
            .ok_or(hearth_rpc::ResourceError::Unavailable)?;

        let api = ProcessApiImpl {
            store: self.store.to_owned(),
            wrapper,
        };

        let api = Arc::new(api);
        let (server, client) = ProcessApiServerShared::<_, remoc::codec::Default>::new(api, 128);

        tokio::spawn(async move {
            server.serve(true).await;
        });

        Ok(client)
    }

    async fn register_service(&self, pid: LocalProcessId, name: String) -> ResourceResult<()> {
        let cap = self
            .factory
            .get_pid_wrapper(pid)
            .ok_or(hearth_rpc::ResourceError::Unavailable)?
            .cap;

        if let Some(old) = self.registry.insert(name, cap) {
            old.free(self.store.as_ref());
        }

        Ok(())
    }

    async fn deregister_service(&self, name: String) -> ResourceResult<()> {
        if let Some(old) = self.registry.remove(name) {
            old.free(self.store.as_ref());
            Ok(())
        } else {
            Err(ResourceError::Unavailable)
        }
    }

    async fn follow_process_list(
        &self,
    ) -> CallResult<HashMapSubscription<LocalProcessId, ProcessStatus>> {
        Ok(self.factory.statuses.read().subscribe(1024))
    }

    async fn follow_service_list(&self) -> CallResult<HashMapSubscription<String, LocalProcessId>> {
        Err(remoc::rtc::CallError::RemoteForward)
    }
}

pub struct ProcessApiImpl<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    wrapper: ProcessWrapper,
}

#[async_trait]
impl<Store> hearth_rpc::ProcessApi for ProcessApiImpl<Store>
where
    Store: ProcessStoreTrait + Send + Sync,
{
    async fn is_alive(&self) -> CallResult<bool> {
        Ok(self.store.is_alive(self.wrapper.cap.get_handle()))
    }

    async fn kill(&self) -> CallResult<()> {
        self.store.kill(self.wrapper.cap.get_handle());
        Ok(())
    }

    async fn follow_log(&self) -> CallResult<ListSubscription<ProcessLogEvent>> {
        Ok(self.wrapper.log_distributor.subscribe())
    }
}

pub struct ProcessFactoryImpl<Store> {
    factory: Arc<ProcessFactory<Store>>,
}

#[async_trait]
impl<Store> hearth_rpc::ProcessFactory for ProcessFactoryImpl<Store>
where
    Store: ProcessStoreTrait + Send + Sync,
{
    async fn spawn(&self, _process: ProcessBase) -> CallResult<ProcessOffer> {
        Err(remoc::rtc::CallError::RemoteForward)
    }
}

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

use hearth_rpc::*;
use hearth_rpc::hearth_types::LocalProcessId;
use remoc::robs::hash_map::HashMapSubscription;
use remoc::rtc::async_trait;
use tracing::info;

use super::Flags;
use super::factory::ProcessFactory;
use super::local::LocalProcess;
use super::registry::Registry;
use super::store::{Capability, ProcessStoreTrait};

pub struct ProcessStoreImpl<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    factory: ProcessFactory<Store>,
    registry: Registry<Store>,
}

#[async_trait]
impl<Store> hearth_rpc::ProcessStore for ProcessStoreImpl<Store>
where
    Store: ProcessStoreTrait + Send + Sync,
    Store::Entry: From<LocalProcess>,
{
    async fn print_hello_world(&self) -> CallResult<()> {
        info!("Hello, world!");
        Ok(())
    }

    async fn find_process(&self, _pid: LocalProcessId) -> ResourceResult<ProcessApiClient> {
        Err(hearth_rpc::ResourceError::Unavailable)
    }

    async fn register_service(&self, pid: LocalProcessId, name: String) -> ResourceResult<()> {
        let handle = self
            .factory
            .get_pid_handle(pid)
            .ok_or(hearth_rpc::ResourceError::Unavailable)?;

        let old = self.registry.insert(
            name,
            &Capability {
                handle,
                flags: Flags,
            },
        );

        if let Some(old) = old {
            self.store.dec_ref(old.handle);
        }

        Ok(())
    }

    async fn deregister_service(&self, name: String) -> ResourceResult<()> {
        if let Some(old) = self.registry.remove(name) {
            self.store.dec_ref(old.handle);
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

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

use std::ops::{Deref, DerefMut};
use std::sync::Arc;

use hearth_rpc::hearth_types::{LocalProcessId, PeerId, ProcessId, ProcessLogLevel};
use hearth_rpc::remoc::robs::hash_map::ObservableHashMap;
use hearth_rpc::remoc::robs::list::ObservableListDistributor;
use hearth_rpc::{remoc, ProcessInfo, ProcessLogEvent, ProcessStatus};
use parking_lot::RwLock;
use remoc::rch::watch;
use remoc::robs::list::ObservableList;
use slab::Slab;
use tokio::sync::mpsc::unbounded_channel;

use super::local::{LocalProcess, ProcessContext};
use super::store::{Capability, ProcessStoreTrait};
use super::Flags;

struct ProcessWrapper {
    info: ProcessInfo,
    handle: usize,
    log_distributor: ObservableListDistributor<ProcessLogEvent>,
}

pub struct ProcessFactory<Store> {
    store: Arc<Store>,
    peer: PeerId,
    processes: RwLock<Slab<ProcessWrapper>>,
    statuses: RwLock<ObservableHashMap<LocalProcessId, ProcessStatus>>,
}

impl<Store> ProcessFactory<Store>
where
    Store: ProcessStoreTrait,
    Store::Entry: From<LocalProcess>,
{
    /// Spawns a process.
    pub fn spawn(&self, info: ProcessInfo, flags: Flags) -> Process<Store> {
        let log = ObservableList::new();
        let (mailbox_tx, mailbox) = unbounded_channel();
        let entry = LocalProcess { mailbox_tx };
        let handle = self.store.insert(entry.into());
        let (warning_num_tx, warning_num) = watch::channel(0);
        let (error_num_tx, error_num) = watch::channel(0);
        let (log_num_tx, log_num) = watch::channel(0);

        let pid = LocalProcessId(self.processes.write().insert(ProcessWrapper {
            info: info.clone(),
            handle, // TODO refcounting on process factory entries?
            log_distributor: log.distributor(),
        }) as u32);

        self.statuses.write().insert(
            pid,
            ProcessStatus {
                warning_num,
                error_num,
                log_num,
                info,
            },
        );

        let self_cap = Capability { handle, flags };
        let ctx = ProcessContext::new(self.store.to_owned(), self_cap, mailbox);

        Process {
            pid: ProcessId::from_peer_process(self.peer, pid),
            ctx,
            log,
            warning_num_tx,
            error_num_tx,
            log_num_tx,
        }
    }

    // TODO figure out visibility for this
    fn get_pid_handle(&self, pid: LocalProcessId) -> Option<usize> {
        self.processes.read().get(pid.0 as usize).map(|w| w.handle)
    }
}

/// An owned, local process.
pub struct Process<Store: ProcessStoreTrait> {
    /// The ID of this process.
    pid: ProcessId,

    /// The context for this process.
    ctx: ProcessContext<Store>,

    /// Observable log for this process's log events.
    log: ObservableList<ProcessLogEvent>,

    /// A sender to this process's number of warning logs.
    warning_num_tx: watch::Sender<u32>,

    /// A sender to this process's number of error logs.
    error_num_tx: watch::Sender<u32>,

    /// A sender to this process's total number of log events.
    log_num_tx: watch::Sender<u32>,
}

impl<Store: ProcessStoreTrait> Deref for Process<Store> {
    type Target = ProcessContext<Store>;

    fn deref(&self) -> &Self::Target {
        &self.ctx
    }
}

impl<Store: ProcessStoreTrait> DerefMut for Process<Store> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.ctx
    }
}

impl<Store: ProcessStoreTrait> Process<Store> {
    /// Gets the [ProcessId] for this process.
    pub fn get_pid(&self) -> ProcessId {
        self.pid
    }

    /// Adds a lot event to this process's log.
    pub fn log(&mut self, event: ProcessLogEvent) {
        // helper function for incrementing watched counter
        let inc_num = |watch: &mut watch::Sender<u32>| {
            watch.send_modify(|i| *i += 1);
        };

        // update level-specific log event counters
        match event.level {
            ProcessLogLevel::Warning => inc_num(&mut self.warning_num_tx),
            ProcessLogLevel::Error => inc_num(&mut self.error_num_tx),
            _ => {}
        }

        // always increment the total log event counter
        inc_num(&mut self.log_num_tx);

        // actually push the log event
        self.log.push(event);
    }
}

pub mod rpc {
    use super::*;

    use hearth_rpc::{CallResult, ProcessApiClient, ResourceError, ResourceResult};
    use remoc::robs::hash_map::HashMapSubscription;
    use remoc::rtc::async_trait;
    use tracing::info;

    use crate::process::registry::Registry;

    pub struct ProcessStoreImpl<Store: ProcessStoreTrait> {
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

            self.registry.insert(
                name,
                &Capability {
                    handle,
                    flags: Flags,
                },
            );

            Ok(())
        }

        async fn deregister_service(&self, name: String) -> ResourceResult<()> {
            if let Some(old) = self.registry.remove(name) {
                self.factory.store.dec_ref(old.handle);
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

        async fn follow_service_list(
            &self,
        ) -> CallResult<HashMapSubscription<String, LocalProcessId>> {
            Err(remoc::rtc::CallError::RemoteForward)
        }
    }
}

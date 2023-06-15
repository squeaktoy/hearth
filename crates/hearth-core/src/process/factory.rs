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

use super::context::{Capability, Flags, ProcessContext};
use super::local::LocalProcess;
use super::store::ProcessStoreTrait;

pub(crate) struct ProcessWrapper {
    pub info: ProcessInfo,
    pub cap: Capability,
    pub log_distributor: ObservableListDistributor<ProcessLogEvent>,
}

pub struct ProcessFactory<Store> {
    store: Arc<Store>,
    peer: PeerId,
    processes: RwLock<Slab<ProcessWrapper>>,
    pub(crate) statuses: RwLock<ObservableHashMap<LocalProcessId, ProcessStatus>>,
}

impl<Store> ProcessFactory<Store>
where
    Store: ProcessStoreTrait,
    Store::Entry: From<LocalProcess>,
{
    /// Creates a new process factory.
    pub fn new(store: Arc<Store>, peer: PeerId) -> Self {
        Self {
            store,
            peer,
            processes: Default::default(),
            statuses: Default::default(),
        }
    }

    /// Spawns a process.
    pub fn spawn(&self, info: ProcessInfo, flags: Flags) -> Process<Store> {
        let log = ObservableList::new();
        let (mailbox_tx, mailbox) = unbounded_channel();
        let entry = LocalProcess { mailbox_tx };
        let handle = self.store.insert(entry.into());
        let (warning_num_tx, warning_num) = watch::channel(0);
        let (error_num_tx, error_num) = watch::channel(0);
        let (log_num_tx, log_num) = watch::channel(0);
        let self_cap = Capability::new(handle, flags);

        let pid = LocalProcessId(self.processes.write().insert(ProcessWrapper {
            info: info.clone(),
            cap: self_cap.clone(self.store.as_ref()),
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

    pub(crate) fn get_pid_wrapper(&self, pid: LocalProcessId) -> Option<ProcessWrapper> {
        self.processes
            .read()
            .get(pid.0 as usize)
            .map(|wrapper| ProcessWrapper {
                info: wrapper.info.clone(),
                cap: wrapper.cap.clone(self.store.as_ref()),
                log_distributor: wrapper.log_distributor.clone(),
            })
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

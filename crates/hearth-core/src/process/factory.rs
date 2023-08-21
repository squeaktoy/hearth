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

use hearth_types::{Flags, ProcessId, ProcessLogLevel};
use parking_lot::RwLock;
use slab::Slab;
use tokio::sync::mpsc::unbounded_channel;
use tracing::debug;

use super::context::{Capability, ProcessContext};
use super::local::LocalProcess;
use super::registry::Registry;
use super::store::ProcessStoreTrait;

/// User-frinedly metadata for a local process.
#[derive(Clone, Debug, Hash)]
pub struct ProcessInfo {}

/// Log event emitted by a process.
#[derive(Clone, Debug, Hash)]
pub struct ProcessLogEvent {
    pub level: ProcessLogLevel,
    pub module: String,
    pub content: String,
    // TODO optional source code location?
    // TODO serializeable timestamp?
}

pub(crate) struct ProcessWrapper {
    pub info: ProcessInfo,
    pub cap: Capability,
}

pub struct ProcessFactory<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    registry: Arc<Registry<Store>>,
    processes: RwLock<Slab<ProcessWrapper>>,
}

impl<Store: ProcessStoreTrait> Drop for ProcessFactory<Store> {
    fn drop(&mut self) {
        let processes = std::mem::take(&mut self.processes).into_inner();
        for (_, process) in processes {
            process.cap.free(self.store.as_ref());
        }
    }
}

impl<Store> ProcessFactory<Store>
where
    Store: ProcessStoreTrait,
    Store::Entry: From<LocalProcess>,
{
    /// Creates a new process factory.
    pub fn new(store: Arc<Store>, registry: Arc<Registry<Store>>) -> Self {
        Self {
            store,
            registry,
            processes: Default::default(),
        }
    }

    /// Spawns a process.
    pub fn spawn(&self, info: ProcessInfo, flags: Flags) -> Process<Store> {
        let (mailbox_tx, mailbox) = unbounded_channel();
        let entry = LocalProcess { mailbox_tx };
        let handle = self.store.insert(entry.into());
        let self_cap = Capability::new(handle, flags);

        let pid = ProcessId(self.processes.write().insert(ProcessWrapper {
            info: info.clone(),
            cap: self_cap.clone(self.store.as_ref()),
        }) as u32);

        let ctx = ProcessContext::new(self.store.to_owned(), self_cap, mailbox);

        Process {
            pid,
            ctx,
            registry: self.registry.clone(),
        }
    }

    pub(crate) fn get_pid_wrapper(&self, pid: ProcessId) -> Option<ProcessWrapper> {
        self.processes
            .read()
            .get(pid.0 as usize)
            .map(|wrapper| ProcessWrapper {
                info: wrapper.info.clone(),
                cap: wrapper.cap.clone(self.store.as_ref()),
            })
    }
}

/// An owned, local process.
pub struct Process<Store: ProcessStoreTrait> {
    /// The ID of this process.
    pid: ProcessId,

    /// The context for this process.
    ctx: ProcessContext<Store>,

    /// The registry for this process.
    registry: Arc<Registry<Store>>,
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
        debug!("process log: {:?}", event);
    }

    /// Retrieves a service capability from the registry.
    pub fn get_service(&mut self, name: impl AsRef<str>) -> Option<usize> {
        self.registry.get(name).map(|cap| self.ctx.insert_cap(cap))
    }
}

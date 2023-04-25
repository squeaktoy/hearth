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

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use sharded_slab::Slab as ShardedSlab;

use super::local::LocalProcess;
use super::Flags;

/// An interface trait for implementations of a process store. See the module-level docs for more info.
pub trait ProcessStoreTrait {
    type Entry: ProcessEntry;

    fn insert(&self, process: Self::Entry) -> usize;

    fn send(&self, handle: usize, message: Message);

    /// Kills a process by its handle.
    ///
    /// This is always assumed to work, so all calls to [Self::is_alive] will
    /// return false after this.
    ///
    /// Killing a process with the same handle twice is defined behavior and
    /// does nothing.
    fn kill(&self, handle: usize);

    fn is_alive(&self, handle: usize) -> bool;

    fn inc_ref(&self, handle: usize);

    fn dec_ref(&self, handle: usize);
}

struct ProcessWrapper<Process> {
    inner: Process,
    is_alive: AtomicBool,
    ref_count: AtomicUsize,
}

pub struct ProcessStore<Entry: ProcessEntry> {
    /// A sharded slab of the process entries in this store.
    entries: ShardedSlab<ProcessWrapper<Entry>>,

    /// The data stored along with this store's entries.
    ///
    /// See [ProcessEntry::Data] for more info.
    entries_data: Entry::Data,
}

impl<Entry: ProcessEntry> ProcessStoreTrait for ProcessStore<Entry> {
    type Entry = Entry;

    fn insert(&self, process: Self::Entry) -> usize {
        let entry = self
            .entries
            .vacant_entry()
            .expect("process store at capacity");
        let handle = entry.key();
        process.on_insert(&self.entries_data, handle);
        entry.insert(ProcessWrapper {
            inner: process,
            is_alive: AtomicBool::new(true),
            ref_count: AtomicUsize::new(1),
        });

        handle
    }

    fn send(&self, handle: usize, message: Message) {
        self.get(handle).inner.on_send(&self.entries_data, message);
    }

    fn kill(&self, handle: usize) {
        self.get(handle).inner.on_kill(&self.entries_data);
    }

    fn is_alive(&self, handle: usize) -> bool {
        self.get(handle).is_alive.load(Ordering::Relaxed)
    }

    fn inc_ref(&self, handle: usize) {
        self.get(handle).ref_count.fetch_add(1, Ordering::Release);
    }

    fn dec_ref(&self, handle: usize) {
        let process = self.get(handle);
        if process.ref_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            process.inner.on_remove(&self.entries_data);
            self.entries.remove(handle);
        }
    }
}

impl<T: ProcessEntry> ProcessStore<T> {
    /// Internal utility function for retrieving a valid handle. Panics if the handle is invalid.
    fn get(&self, handle: usize) -> impl std::ops::Deref<Target = ProcessWrapper<T>> + '_ {
        self.entries.get(handle).expect("invalid handle")
    }
}

/// A trait for all processes stored in a process store.
pub trait ProcessEntry {
    /// The global process data for this process entry type. All methods on
    /// this entry use the data for the store the entry is in.
    type Data;

    /// Called when this entry is first inserted into the store.
    fn on_insert(&self, data: &Self::Data, handle: usize);

    /// Called when a message is sent to this entry.
    ///
    /// All message capabilities are in the scope of the owned store, and all
    /// capabilities are already ref-counted with this message, so when the
    /// message is freed, all references need to freed too.
    fn on_send(&self, data: &Self::Data, message: Message);

    /// Called when this entry is killed.
    fn on_kill(&self, data: &Self::Data);

    /// Called when this entry is removed from the store.
    ///
    /// Note that [Self::on_kill] is called when a process being removed was
    /// still alive, so there's no need to account for still-alive functions
    /// here.
    fn on_remove(&self, data: &Self::Data);
}

/// A message sent to a process.
///
/// All handles are scoped within a process store.
#[derive(Clone, Debug)]
pub enum Message {
    /// Sent when a linked process has been unlinked.
    Unlink {
        /// The handle to the unlinked process within the process store.
        subject: usize,
    },
    /// A message containing a data payload and transferred capabilities.
    Data {
        /// The data payload of this message.
        data: Vec<u8>,

        /// The list of capabilities transferred with this message.
        ///
        /// These capabilities are non-owning. Before this message is dropped,
        /// all capability refs need to be either cleaned up or stored
        /// somewhere else.
        caps: Vec<Capability>,
    },
}

/// A capability within a process store.
///
/// This capability is non-owning.
#[derive(Copy, Clone, Debug)]
pub struct Capability {
    /// The handle of the target process within the process store.
    pub handle: usize,

    /// The permission flags associated with this capability.
    pub flags: Flags,
}

impl !Drop for Capability {}

pub struct AnyProcessData {
    pub local: <LocalProcess as ProcessEntry>::Data,
}

/// A process entry that can be either remote or local.
pub enum AnyProcess {
    Local(LocalProcess),
}

impl ProcessEntry for AnyProcess {
    type Data = AnyProcessData;

    fn on_insert(&self, data: &Self::Data, handle: usize) {
        match self {
            AnyProcess::Local(local) => local.on_insert(&data.local, handle),
        }
    }

    fn on_send(&self, data: &Self::Data, message: Message) {
        match self {
            AnyProcess::Local(local) => local.on_send(&data.local, message),
        }
    }

    fn on_kill(&self, data: &Self::Data) {
        match self {
            AnyProcess::Local(local) => local.on_kill(&data.local),
        }
    }

    fn on_remove(&self, data: &Self::Data) {
        match self {
            AnyProcess::Local(local) => local.on_remove(&data.local),
        }
    }
}

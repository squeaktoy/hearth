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

//! Low-level process storage.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use parking_lot::Mutex;
use sharded_slab::Slab as ShardedSlab;
use tracing::trace;

use super::context::Capability;
use super::local::LocalProcess;

/// An interface trait implemented by all process stores.
///
/// Process stores contain all of the processes accessible on a local peer.
/// There is not a strict 1-to-1 correlation between process entries in this
/// store and a single instance of a process. For example, a remote process may
/// have multiple entries in this store because the remote peer has offered the
/// local peer multiple capabilities to the same remote process.
///
/// Process entries are referenced by their handle, which is a
/// non-human-readable `usize`. Process entries in the local store are
/// reference-counted. This ensures that all references to a process stay valid
/// as long as they are needed, even if a process is killed or forcefully
/// revoked by a remote peer. All process entries are valid until all
/// references have been dropped. The reference count is manually modified by
/// two methods: [ProcessStoreTrait::inc_ref] and [ProcessStoreTrait::dec_ref],
/// which respectively increment and decrement the reference count of a handle.
/// Once the reference count of a handle is
/// decremented to 0, the process entry is destroyed, and the handle becomes
/// invalid. All methods panic if given an invalid handle.
///
/// Each process entry can be either alive or dead. Dead processes do not
/// receive any messages sent to them. Please note that even if a process entry
/// is dead, its handle is still valid as long as it still has a non-zero
/// reference count. Every process entry can be killed using its handle with
/// this trait, because the mechanism of killing processes lives in the process
/// store but the policy of when processes die and who kills them is determined
/// elsewhere.
pub trait ProcessStoreTrait {
    type Entry: ProcessEntry;

    /// Inserts a process entry into the store and returns a new handle to it.
    ///
    /// The reference count starts at 1, so this handle is owning. After
    /// calling this, you probably want to turn the handle into a capability
    /// with [Capability::new].
    fn insert(&self, process: Self::Entry) -> usize;

    /// Sends a message to this process.
    ///
    /// Does nothing if the process is dead.
    fn send(&self, handle: usize, message: Message);

    /// Kills a process by its handle.
    ///
    /// This is always assumed to work, so all calls to [Self::is_alive] will
    /// return false after this.
    ///
    /// Killing a process with the same handle twice is defined behavior but
    /// does nothing.
    fn kill(&self, handle: usize);

    /// Links the subject process to the object process.
    ///
    /// When the subject process dies, the store will send a [Message::Unlink]
    /// message to the object process. The message is sent immediately if the
    /// link subject is already dead.
    fn link(&self, subject: usize, object: usize);

    /// Tests if a process is alive or not.
    ///
    /// Like the other methods, still panics if given an invalid handle.
    fn is_alive(&self, handle: usize) -> bool;

    /// Increments the reference count to a handle.
    fn inc_ref(&self, handle: usize);

    /// Decrements the reference count to a handle.
    ///
    /// When the reference count is decremented to 0, the handle becomes
    /// and the associated entry gets removed from the store.
    fn dec_ref(&self, handle: usize);
}

struct ProcessWrapper<Process> {
    inner: Process,
    is_alive: AtomicBool,
    linked: Mutex<Vec<usize>>,
    ref_count: AtomicUsize,
}

/// The canonical [ProcessStoreTrait] implementation.
///
/// This struct implements [ProcessStoreTrait] for any generic [ProcessEntry].
pub struct ProcessStore<Entry: ProcessEntry> {
    /// A sharded slab of the process entries in this store.
    entries: ShardedSlab<ProcessWrapper<Entry>>,

    /// The data stored along with this store's entries.
    ///
    /// See [ProcessEntry::Data] for more info.
    entries_data: Entry::Data,
}

impl<Entry: ProcessEntry> Default for ProcessStore<Entry>
where
    Entry::Data: Default,
{
    fn default() -> Self {
        Self {
            entries: ShardedSlab::new(),
            entries_data: Default::default(),
        }
    }
}

impl<Entry: ProcessEntry> ProcessStoreTrait for ProcessStore<Entry> {
    type Entry = Entry;

    fn insert(&self, process: Self::Entry) -> usize {
        let entry = self
            .entries
            .vacant_entry()
            .expect("process store at capacity");
        let handle = entry.key();
        trace!("inserting process {}", handle);
        process.on_insert(&self.entries_data, handle);
        entry.insert(ProcessWrapper {
            inner: process,
            is_alive: AtomicBool::new(true),
            linked: Default::default(),
            ref_count: AtomicUsize::new(1),
        });

        handle
    }

    fn send(&self, handle: usize, message: Message) {
        trace!("sending to process {}", handle);
        self.get(handle).inner.on_send(&self.entries_data, message);
    }

    fn kill(&self, handle: usize) {
        trace!("killing process {}", handle);
        let entry = self.get(handle);
        if entry.is_alive.swap(false, Ordering::SeqCst) {
            entry.inner.on_kill(&self.entries_data);

            for link in entry.linked.lock().drain(..) {
                self.send(link, Message::Unlink { subject: handle });
            }
        }
    }

    fn link(&self, subject: usize, object: usize) {
        trace!("linking subject {} to object {}", subject, object);
        let entry = self.get(subject);
        let mut linked = entry.linked.lock();
        self.inc_ref(object);
        linked.push(object);
    }

    fn is_alive(&self, handle: usize) -> bool {
        trace!("testing if process {} is alive", handle);
        self.get(handle).is_alive.load(Ordering::Relaxed)
    }

    fn inc_ref(&self, handle: usize) {
        trace!("incrementing process {} refcount", handle);
        self.get(handle).ref_count.fetch_add(1, Ordering::Release);
    }

    fn dec_ref(&self, handle: usize) {
        trace!("decrementing process {} refcount", handle);
        let process = self.get(handle);
        if process.ref_count.fetch_sub(1, Ordering::SeqCst) == 1 {
            trace!("removing process {}", handle);
            process.inner.on_remove(&self.entries_data);

            for link in process.linked.lock().iter() {
                self.dec_ref(*link);
            }

            self.entries.remove(handle);
        }
    }
}

impl<T: ProcessEntry> ProcessStore<T> {
    /// Creates a new, empty process store with the given entry data.
    pub fn new(data: T::Data) -> Self {
        Self {
            entries: ShardedSlab::new(),
            entries_data: data,
        }
    }

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
#[derive(Debug, PartialEq, Eq)]
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

impl Message {
    /// Duplicates this messages and increments its reference counts.
    pub fn clone(&self, store: &impl ProcessStoreTrait) -> Self {
        use Message::*;
        match self {
            Unlink { subject } => {
                let subject = *subject;
                store.inc_ref(subject);
                Unlink { subject }
            }
            Data { data, caps } => Data {
                data: data.to_owned(),
                caps: caps.iter().map(|cap| cap.clone(store)).collect(),
            },
        }
    }

    /// Safely frees this message and any references within the store.
    pub fn free(self, store: &impl ProcessStoreTrait) {
        use Message::*;
        match self {
            Unlink { subject } => {
                store.inc_ref(subject);
            }
            Data { caps, .. } => {
                for cap in caps {
                    cap.free(store);
                }
            }
        }
    }
}

#[derive(Default)]
pub struct AnyProcessData {
    pub local: <LocalProcess as ProcessEntry>::Data,
}

/// A process entry that can be either remote or local.
pub enum AnyProcess {
    Local(LocalProcess),
}

impl From<LocalProcess> for AnyProcess {
    fn from(local: LocalProcess) -> Self {
        AnyProcess::Local(local)
    }
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

#[cfg(test)]
pub mod tests {
    use super::*;

    use std::sync::mpsc::{channel, Receiver, Sender};

    use crate::process::context::Flags;

    pub struct MockProcessEntry {
        mailbox_tx: Sender<Message>,
    }

    impl ProcessEntry for MockProcessEntry {
        type Data = ();

        fn on_insert(&self, _data: &Self::Data, handle: usize) {
            eprintln!("on_insert(handle = {})", handle);
        }

        fn on_send(&self, _data: &Self::Data, message: Message) {
            eprintln!("on_send(message = {:?})", message);
            let _ = self.mailbox_tx.send(message);
        }

        fn on_kill(&self, _data: &Self::Data) {
            eprintln!("on_kill()");
        }

        fn on_remove(&self, _data: &Self::Data) {
            eprintln!("on_remove()");
        }
    }

    impl ProcessStore<MockProcessEntry> {
        /// Internal utility function for testing if a handle is valid.
        pub fn contains(&self, handle: usize) -> bool {
            self.entries.contains(handle)
        }

        /// Helper function to insert a mock process entry into a store.
        pub fn insert_mock(&self) -> usize {
            let (mailbox_tx, _mailbox) = channel();
            self.insert(MockProcessEntry { mailbox_tx })
        }

        /// Helper function to insert a mock process that forwards messages.
        pub fn insert_forward(&self) -> (Receiver<Message>, usize) {
            let (mailbox_tx, mailbox) = channel();
            let handle = self.insert(MockProcessEntry { mailbox_tx });
            (mailbox, handle)
        }
    }

    /// Helper function to create an empty mock process store.
    pub fn make_store() -> ProcessStore<MockProcessEntry> {
        ProcessStore::new(())
    }

    #[test]
    fn create_store() {
        let _store = make_store();
    }

    #[test]
    fn send() {
        let store = make_store();
        let (mailbox, handle) = store.insert_forward();

        let message = Message::Data {
            data: b"Hello, world!".to_vec(),
            caps: vec![],
        };

        store.send(handle, message.clone(&store));
        assert_eq!(mailbox.try_recv(), Ok(message));
    }

    #[test]
    fn send_dead() {
        let store = make_store();
        let (mailbox, handle) = store.insert_forward();

        store.kill(handle);

        store.send(
            handle,
            Message::Data {
                data: vec![],
                caps: vec![],
            },
        );

        assert!(mailbox.try_recv().is_err());
    }

    #[test]
    fn link() {
        let store = make_store();
        let subject = store.insert_mock();
        let (mailbox, object) = store.insert_forward();
        store.link(subject, object);
        store.kill(subject);
        assert_eq!(mailbox.try_recv(), Ok(Message::Unlink { subject }));
    }

    #[test]
    fn link_dead() {
        let store = make_store();
        let subject = store.insert_mock();
        let (mailbox, object) = store.insert_forward();
        store.kill(subject);
        store.link(subject, object);
        assert_eq!(mailbox.try_recv(), Ok(Message::Unlink { subject }));
    }

    #[test]
    fn ref_counting() {
        let store = make_store();
        let handle = store.insert_mock();
        assert!(store.contains(handle));
        store.dec_ref(handle);
        assert!(!store.contains(handle));
    }

    #[test]
    fn kill() {
        let store = make_store();
        let handle = store.insert_mock();
        assert!(store.is_alive(handle));
        store.kill(handle);
        assert!(!store.is_alive(handle));
    }

    #[test]
    fn double_kill() {
        let store = make_store();
        let handle = store.insert_mock();
        store.kill(handle);
        store.kill(handle);
    }

    #[test]
    fn link_object_holds_reference() {
        let store = make_store();
        let subject = store.insert_mock();
        let object = store.insert_mock();
        store.link(subject, object);
        store.dec_ref(subject);
        assert!(store.contains(subject));
        store.dec_ref(object);
        assert!(!store.contains(subject));
    }

    #[test]
    fn link_subject_holds_reference() {
        let store = make_store();
        let subject = store.insert_mock();
        let object = store.insert_mock();
        store.link(subject, object);
        store.dec_ref(object);
        assert!(store.contains(object));
        store.dec_ref(subject);
        assert!(!store.contains(subject));
    }

    #[test]
    fn cyclic_linking_deref() {
        let store = make_store();
        let a = store.insert_mock();
        let b = store.insert_mock();
        store.link(a, b);
        store.link(b, a);
        store.dec_ref(a);
        store.dec_ref(b);
        assert!(!store.contains(a));
        assert!(!store.contains(b));
    }

    #[test]
    fn no_double_linking() {
        let store = make_store();
        let subject = store.insert_mock();
        let (mailbox, object) = store.insert_forward();
        store.link(subject, object);
        store.link(subject, object);
        store.kill(subject);
        assert_eq!(mailbox.try_recv(), Ok(Message::Unlink { subject }));
        assert!(mailbox.try_recv().is_err());
    }

    #[test]
    fn safe_message_drop() {
        let store = make_store();
        let handle = store.insert_mock();

        let message = Message::Data {
            data: vec![],
            caps: vec![Capability::new(handle, Flags::empty())],
        };

        store.send(handle, message);
        assert!(!store.contains(handle));
    }
}

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

use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::{bail, Context};
use slab::Slab;
use tokio::sync::mpsc::UnboundedReceiver;

use super::store::{Message, ProcessStoreTrait, Signal};

pub use hearth_rpc::hearth_types::Flags;

/// A capability within a process store, storing both a handle and its
/// permission flags.
///
/// This capability is reference-counted but does not own a reference to the
/// store the handle is from. When done using a capability, you need to call
/// [Capability::free] to remove this capability's reference from its store.
/// Capabilities can be duplicated using [Capability::clone], which creates an
/// identical capability and increments the underlying handle's reference
/// count.
///
/// To help write safe and secure capability code, capabilities cannot be
/// dropped without calling [Capability::free]. Rust does not provide a way to
/// make a type un-droppable, so instead [Capability] simply panics in its
/// [Drop] implementation. Unfreed, dropped capabilities will be caught in our
/// unit tests, so we can discover handle leaks without needing to scrutinize
/// every possible capability duplication or change of ownership.
#[derive(Debug, PartialEq, Eq)]
pub struct Capability {
    /// The handle of the target process within the process store.
    handle: usize,

    /// The permission flags associated with this capability.
    flags: Flags,
}

impl Drop for Capability {
    fn drop(&mut self) {
        panic!("capability {} was dropped without freeing", self.handle);
    }
}

impl Capability {
    /// Crate-internal constructor for capabilities.
    ///
    /// The given handle is assumed to already have a counted reference, so
    /// passing a store is unnecessary.
    pub(crate) fn new(handle: usize, flags: Flags) -> Self {
        Self { handle, flags }
    }

    /// Retrieves the handle to the process entry within the store.
    pub(crate) fn get_handle(&self) -> usize {
        self.handle
    }

    /// Retrieves this capability's flags.
    pub(crate) fn get_flags(&self) -> Flags {
        self.flags
    }

    /// Duplicates this capability and increments its reference count in the store.
    pub fn clone(&self, store: &impl ProcessStoreTrait) -> Self {
        store.inc_ref(self.handle);

        Self {
            handle: self.handle,
            flags: self.flags,
        }
    }

    /// Frees this capability and decrements its reference count in the store.
    pub fn free(self, store: &impl ProcessStoreTrait) {
        store.dec_ref(self.handle);
        std::mem::forget(self);
    }
}

/// A signal sent to a process, contextualized in a [ProcessContext].
///
/// All process handles are indices into the context's capability store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContextSignal {
    /// Sent when a linked process has been unlinked.
    Unlink { subject: usize },
    /// A [ContextMessage].
    Message(ContextMessage),
}

/// A message sent to a process, contextualized in a [ProcessContext].
///
/// All process handles are indices into the context's capability store.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextMessage {
    /// The data payload of this message.
    pub data: Vec<u8>,

    /// The list of capabilities transferred with this message.
    ///
    /// These capabilities are loaded into the context.
    pub caps: Vec<usize>,
}

/// Errors in processes are assumed (under Erlang philosophy) to be
/// unrecoverable, so all methods return [anyhow::Result]. This way, even if
/// errors aren't recoverable, they are at least human-readable.
pub struct ProcessContext<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    self_cap: Option<Capability>,
    caps: Slab<Capability>,
    mailbox: UnboundedReceiver<Signal>,

    /// A list of subjects of unlink messages that have not been received yet.
    unlink_queue: VecDeque<usize>,
}

impl<Store: ProcessStoreTrait> Drop for ProcessContext<Store> {
    fn drop(&mut self) {
        while let Ok(message) = self.mailbox.try_recv() {
            message.free(self.store.as_ref());
        }

        if let Some(self_cap) = self.self_cap.take() {
            self_cap.free(self.store.as_ref());
        }

        for cap in self.caps.drain() {
            cap.free(self.store.as_ref());
        }
    }
}

impl<Store: ProcessStoreTrait> ProcessContext<Store> {
    /// Creates a new process context.
    ///
    /// Visibility is limited to the crate because this should not be manually
    /// instantiated, but we still want it to be easy for unit tests to make
    /// these.
    pub(crate) fn new(
        store: Arc<Store>,
        self_cap: Capability,
        mailbox: UnboundedReceiver<Signal>,
    ) -> Self {
        let mut caps = Slab::with_capacity(1);
        let self_handle = caps.insert(self_cap.clone(store.as_ref()));
        assert_eq!(self_handle, 0, "non-zero self-capability handle");

        Self {
            store,
            self_cap: Some(self_cap),
            caps,
            mailbox,
            unlink_queue: VecDeque::new(),
        }
    }

    /// Copies another process context's self-capability into this context.
    pub fn copy_self_capability(&mut self, other: &Self) -> usize {
        assert!(
            Arc::ptr_eq(&self.store, &other.store),
            "attempt to copy self-capability from a context from another process store"
        );

        let cap = other
            .self_cap
            .as_ref()
            .expect("self-capability has been deinitialized");

        self.insert_cap(cap.clone(self.store.as_ref()))
    }

    /// Receives the next signal sent to this process and maps its
    /// capabilities into this context.
    ///
    /// Returns `None` after killed.
    pub async fn recv(&mut self) -> Option<ContextSignal> {
        loop {
            if let Some(subject) = self.unlink_queue.pop_front() {
                return Some(ContextSignal::Unlink { subject });
            }

            match self.mailbox.recv().await? {
                Signal::Kill => {}
                Signal::Unlink { subject } => {
                    let handles = self
                        .caps
                        .iter()
                        .filter(|(_idx, cap)| cap.get_handle() == subject)
                        .map(|(idx, _cap)| idx);

                    self.unlink_queue.extend(handles);
                }
                Signal::Message(Message { data, caps }) => {
                    return Some(ContextSignal::Message(ContextMessage {
                        data,
                        caps: caps.into_iter().map(|cap| self.caps.insert(cap)).collect(),
                    }));
                }
            }
        }
    }

    /// Sends a message to a process.
    pub fn send(&self, handle: usize, message: ContextMessage) -> anyhow::Result<()> {
        let dst = self
            .get_cap(handle)
            .context("ProcessContext::send() destination")?;

        // TODO write unit test for this
        if !dst.flags.contains(Flags::SEND) {
            bail!("capability does not permit send operation");
        }

        let ContextMessage {
            data,
            caps: ctx_caps,
        } = message;

        let mut caps = Vec::with_capacity(ctx_caps.len());
        for (idx, cap) in ctx_caps.into_iter().enumerate() {
            let store_cap = self
                .get_cap(cap)
                .with_context(|| format!("sending mapped message capability (index #{})", idx))?;
            caps.push(store_cap.clone(self.store.as_ref()));
        }

        self.store.send(dst.get_handle(), Message { data, caps });

        Ok(())
    }

    pub fn kill(&self, handle: usize) -> anyhow::Result<()> {
        let target = self
            .get_cap(handle)
            .context("ProcessContext::kill() target")?;

        // TODO write unit test for this
        if !target.flags.contains(Flags::KILL) {
            bail!("capability does not permit kill operation");
        }

        self.store.kill(target.get_handle());

        Ok(())
    }

    /// Creates a new capability from an existing one, using a subset of the original's flags.
    pub fn make_capability(&mut self, handle: usize, new_flags: Flags) -> anyhow::Result<usize> {
        let original = self
            .get_cap(handle)
            .context("ProcessContext::make_capability() original")?;

        // TODO write unit test for this
        if !original.flags.contains(new_flags) {
            bail!(
                "capability flags cannot be promoted from {:?} to {:?}",
                original.flags,
                new_flags
            );
        }

        let mut cap = original.clone(self.store.as_ref());
        cap.flags = new_flags;
        Ok(self.caps.insert(cap))
    }

    /// Deletes a capability from this context.
    pub fn delete_capability(&mut self, handle: usize) -> anyhow::Result<()> {
        let cap = self
            .caps
            .try_remove(handle)
            .with_context(|| format!("invalid handle {}", handle))?;

        cap.free(self.store.as_ref());

        // remove all unlink messages targeting this handle
        self.unlink_queue.retain(|c| *c != handle);

        Ok(())
    }

    /// Retrieves the flags of a capability.
    pub fn get_capability_flags(&self, handle: usize) -> anyhow::Result<Flags> {
        self.get_cap(handle)
            .context("ProcessContext::get_capability_flags() handle")
            .map(|cap| cap.flags)
    }

    /// Inserts a capability and returns its handle.
    pub(crate) fn insert_cap(&mut self, cap: Capability) -> usize {
        self.caps.insert(cap)
    }

    /// Retrieves a capability by handle.
    pub(crate) fn get_cap(&self, handle: usize) -> anyhow::Result<&Capability> {
        self.caps
            .get(handle)
            .with_context(|| format!("invalid handle {}", handle))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::process::store;
    use store::tests::MockProcessEntry;
    use store::ProcessStore;

    fn make_store() -> Arc<ProcessStore<MockProcessEntry>> {
        Arc::new(store::tests::make_store())
    }

    fn make_ctx_cap(
        store: &Arc<ProcessStore<MockProcessEntry>>,
        flags: Flags,
    ) -> (ProcessContext<ProcessStore<MockProcessEntry>>, Capability) {
        let (sync_mailbox, handle) = store.insert_forward();
        let (mailbox_tx, mailbox) = tokio::sync::mpsc::unbounded_channel();

        std::thread::spawn(move || {
            while let Ok(message) = sync_mailbox.recv() {
                let _ = mailbox_tx.send(message);
            }
        });

        let cap = Capability::new(handle, flags);
        let self_cap = cap.clone(store.as_ref());
        let ctx = ProcessContext::new(store.to_owned(), self_cap, mailbox);
        (ctx, cap)
    }

    fn make_ctx(
        store: &Arc<ProcessStore<MockProcessEntry>>,
        flags: Flags,
    ) -> ProcessContext<ProcessStore<MockProcessEntry>> {
        let (ctx, cap) = make_ctx_cap(&store, flags);
        cap.free(store.as_ref());
        ctx
    }

    #[tokio::test]
    async fn new() {
        let store = make_store();
        let _ctx = make_ctx(&store, Flags::empty());
    }

    #[tokio::test]
    async fn new_two() {
        let store = make_store();
        let _a = make_ctx(&store, Flags::empty());
        let _b = make_ctx(&store, Flags::empty());
    }

    #[tokio::test]
    async fn recv() {
        let store = make_store();
        let (mut ctx, cap) = make_ctx_cap(&store, Flags::SEND);

        let msg = Message {
            data: b"Hello, world!".to_vec(),
            caps: vec![],
        };

        store.send(cap.get_handle(), msg);
        cap.free(store.as_ref());

        assert_eq!(
            ctx.recv().await.unwrap(),
            ContextSignal::Message(ContextMessage {
                data: b"Hello, world!".to_vec(),
                caps: vec![]
            })
        );
    }

    #[tokio::test]
    async fn send() {
        let store = make_store();
        let (mut a_ctx, a_cap) = make_ctx_cap(&store, Flags::SEND);
        let mut b_ctx = make_ctx(&store, Flags::empty());
        let a_handle = b_ctx.insert_cap(a_cap);

        let msg = ContextMessage {
            data: vec![],
            caps: vec![],
        };

        b_ctx.send(a_handle, msg.clone()).unwrap();
        assert_eq!(a_ctx.recv().await, Some(ContextSignal::Message(msg)));
    }

    #[tokio::test]
    async fn send_caps() {
        let store = make_store();
        let (mut a_ctx, a_cap) = make_ctx_cap(&store, Flags::SEND);
        let mut b_ctx = make_ctx(&store, Flags::empty());
        let a_handle = b_ctx.insert_cap(a_cap);

        let msg = ContextMessage {
            data: vec![],
            caps: vec![0], // send self handle
        };

        b_ctx.send(a_handle, msg).unwrap();

        assert_eq!(
            a_ctx.recv().await,
            Some(ContextSignal::Message(ContextMessage {
                data: vec![],
                caps: vec![1], // "a" capability gets loaded at cap index 1
            }))
        );
    }

    #[tokio::test]
    async fn delete_self_cap() {
        let store = make_store();
        let (mut ctx, cap) = make_ctx_cap(&store, Flags::empty());
        let handle = cap.get_handle();
        ctx.delete_capability(0).unwrap();
        assert!(store.contains(handle));
        cap.free(store.as_ref());
        assert!(store.contains(handle));
    }
}

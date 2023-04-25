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
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

use super::store::{Capability, Message, ProcessEntry, ProcessStoreTrait};
use super::Flags;

pub struct LocalProcess {
    pub mailbox_tx: UnboundedSender<Message>,
}

impl ProcessEntry for LocalProcess {
    type Data = ();

    fn on_insert(&self, _data: &Self::Data, _handle: usize) {}

    fn on_send(&self, _data: &Self::Data, message: Message) {
        // TODO send errors
        let _ = self.mailbox_tx.send(message);
    }

    fn on_kill(&self, _data: &Self::Data) {
        // TODO kill without remove?
    }

    fn on_remove(&self, _data: &Self::Data) {}
}

/// A message sent to a process, contextualized in a [ProcessContext].
///
/// All process handles are indices into the context's capability store.
#[derive(Clone, Debug)]
pub enum ContextMessage {
    /// Sent when a linked process has been unlinked.
    Unlink { subject: usize },
    /// A message containing a data payload and transferred capabilities.
    Data {
        /// The data payload of this message.
        data: Vec<u8>,

        /// The list of capabilities transferred with this message.
        ///
        /// These capabilities are loaded into the context.
        caps: Vec<usize>,
    },
}

/// Errors in processes are assumed (under Erlang philosophy) to be
/// unrecoverable, so all methods return [anyhow::Result]. This way, even if
/// errors aren't recoverable, they are at least human-readable.
pub struct ProcessContext<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    caps: Slab<Capability>,
    mailbox: UnboundedReceiver<Message>,

    /// A list of subjects of unlink messages that have not been received yet.
    unlink_queue: VecDeque<usize>,
}

impl<Store: ProcessStoreTrait> Drop for ProcessContext<Store> {
    fn drop(&mut self) {
        let caps = std::mem::replace(&mut self.caps, Default::default());
        for (_idx, cap) in caps {
            self.store.dec_ref(cap.handle);
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
        mailbox: UnboundedReceiver<Message>,
    ) -> Self {
        let mut caps = Slab::with_capacity(1);
        let self_cap = caps.insert(self_cap);
        assert_eq!(self_cap, 0, "non-zero self-capability handle");

        Self {
            store,
            caps,
            mailbox,
            unlink_queue: VecDeque::new(),
        }
    }

    /// Receives the next mailbox sent to this process and maps its
    /// capabilities into this context.
    ///
    /// Returns `None` after killed.
    pub async fn recv(&mut self) -> Option<ContextMessage> {
        loop {
            if let Some(subject) = self.unlink_queue.pop_front() {
                return Some(ContextMessage::Unlink { subject });
            }

            match self.mailbox.recv().await? {
                Message::Unlink { subject } => {
                    let handles = self
                        .caps
                        .iter()
                        .filter(|(_idx, cap)| cap.handle == subject)
                        .map(|(idx, _cap)| idx);

                    self.unlink_queue.extend(handles);
                }
                Message::Data { data, caps } => {
                    return Some(ContextMessage::Data {
                        data,
                        caps: caps
                            .into_iter()
                            // no inc_ref() necessary because messages already hold their refs
                            .map(|cap| self.caps.insert(cap))
                            .collect(),
                    });
                }
            }
        }
    }

    /// Sends a message to another peer.
    ///
    /// Returns an error if this is called with [ContextMessage::Unlink].
    pub fn send(&self, handle: usize, message: ContextMessage) -> anyhow::Result<()> {
        let dst = self
            .get_cap(handle)
            .context("ProcessContext::send() destination")?;

        // TODO check for permissions here

        let (data, ctx_caps) = match message {
            ContextMessage::Unlink { .. } => {
                bail!("ProcessContext::send() called with ContextMessage::Unlink")
            }
            ContextMessage::Data { data, caps } => (data, caps),
        };

        let mut caps = Vec::with_capacity(ctx_caps.len());
        for (idx, cap) in ctx_caps.into_iter().enumerate() {
            let store_cap = self
                .get_cap(cap)
                .with_context(|| format!("sending mapped message capability (index #{})", idx))?;
            // TODO is this right? messages should keep refs to their caps, but does this keep the refcount correct?
            self.store.inc_ref(store_cap.handle);
            caps.push(*store_cap);
        }

        self.store.send(dst.handle, Message::Data { data, caps });

        Ok(())
    }

    pub fn kill(&self, handle: usize) -> anyhow::Result<()> {
        let target = self
            .get_cap(handle)
            .context("ProcessContext::kill() target")?;

        // TODO check for permissions here

        // TODO removing vs killing
        self.store.kill(target.handle);

        Ok(())
    }

    /// Creates a new capability from an existing one, using a subset of the original's flags.
    pub fn make_capability(&mut self, handle: usize, _flags: Flags) -> anyhow::Result<usize> {
        let original = self
            .get_cap(handle)
            .context("ProcessContext::make_capability() original")?;

        // TODO calculate subset of flags

        // TODO ProcessEntry::on_new_capability()?

        self.store.inc_ref(original.handle);
        Ok(self.caps.insert(*original))
    }

    /// Deletes a capability from this context.
    pub fn delete_capability(&mut self, handle: usize) -> anyhow::Result<()> {
        let cap = self
            .caps
            .try_remove(handle)
            .with_context(|| format!("invalid handle {}", handle))?;

        self.store.dec_ref(cap.handle);

        // remove all unlink messages targeting this handle
        self.unlink_queue.retain(|c| *c != handle);

        Ok(())
    }

    fn get_cap(&self, handle: usize) -> anyhow::Result<&Capability> {
        self.caps
            .get(handle)
            .with_context(|| format!("invalid handle {}", handle))
    }
}

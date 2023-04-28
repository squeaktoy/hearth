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

use tokio::sync::mpsc::UnboundedSender;

use super::store::{Message, ProcessEntry};

/// A local process entry in a process store.
///
/// This simply forwards messages through an async channel, to be used by other
/// asynchronous tasks.
pub struct LocalProcess {
    /// The mailbox channel sender. Sends all incoming messages to this
    /// process.
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

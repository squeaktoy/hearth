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

use super::store::{ProcessEntry, Signal};

/// A local process entry in a process store.
///
/// This simply forwards signals through an async channel, to be used by other
/// asynchronous tasks.
pub struct LocalProcess {
    /// The mailbox channel sender. Sends all incoming signals to this
    /// process.
    pub mailbox_tx: UnboundedSender<Signal>,
}

impl ProcessEntry for LocalProcess {
    type Data = ();

    fn on_insert(&self, _data: &Self::Data, _handle: usize) {}

    fn on_signal(&self, _data: &Self::Data, signal: Signal) -> Option<Signal> {
        self.mailbox_tx.send(signal).err().map(|err| err.0)
    }

    fn on_remove(&self, _data: &Self::Data) {}
}

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

use flue::{Mailbox, MailboxStore, PostOffice, Table};
use ouroboros::self_referencing;

pub struct ProcessInfo {}

pub struct ProcessFactory {
    post: Arc<PostOffice>,
}

impl ProcessFactory {
    pub fn new(post: Arc<PostOffice>) -> Self {
        Self { post }
    }

    pub fn spawn(&self, info: ProcessInfo) -> Process {
        Process::new(
            Table::new(self.post.clone()),
            |table| MailboxStore::new(table),
            |store| store.create_mailbox().unwrap(),
        )
    }
}

#[self_referencing]
pub struct Process {
    pub table: Table,

    #[borrows(table)]
    #[covariant]
    pub store: MailboxStore<'this>,

    /// A mailbox that receives signals from this process's parent.
    ///
    /// This field lasts the entire lifetime of a process and cannot be dropped.
    /// This is so that local processes can always be killed by their parents
    /// and can't go rogue.
    #[borrows(store)]
    #[covariant]
    pub parent: Mailbox<'this>,
}

impl From<Table> for Process {
    fn from(table: Table) -> Process {
        Process::new(
            table,
            |table| MailboxStore::new(table),
            |store| store.create_mailbox().unwrap(),
        )
    }
}

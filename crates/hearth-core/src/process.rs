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

use std::sync::{atomic::AtomicUsize, Arc};

use flue::{Mailbox, MailboxStore, PostOffice, Table};
use flume::Sender;
use hearth_types::ProcessLogLevel;
use ouroboros::self_referencing;

pub struct ProcessInfo {}

pub struct ProcessFactory {
    post: Arc<PostOffice>,
    pid_gen: AtomicUsize,
}

impl ProcessFactory {
    pub fn new(post: Arc<PostOffice>) -> Self {
        Self {
            post,
            pid_gen: AtomicUsize::new(0),
        }
    }

    /// Spawns a process with an existing [Table].
    pub fn spawn_with_table(&self, _info: ProcessInfo, table: Table) -> Process {
        let pid = self
            .pid_gen
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        let (log_tx, _log_rx) = flume::unbounded();

        Process::new(
            pid,
            log_tx,
            table,
            |table| MailboxStore::new(table),
            |store| store.create_mailbox().unwrap(),
        )
    }

    /// Spawns a new process.
    pub fn spawn(&self, info: ProcessInfo) -> Process {
        self.spawn_with_table(info, Table::new(self.post.clone()))
    }
}

/// Log event emitted by a process.
#[derive(Clone, Debug, Hash)]
pub struct ProcessLogEvent {
    pub level: ProcessLogLevel,
    pub module: String,
    pub content: String,
    // TODO optional source code location?
    // TODO serializeable timestamp?
}

#[self_referencing]
pub struct Process {
    /// The process ID of this process.
    pub pid: usize,

    /// A sender to this process's log.
    pub log_tx: Sender<ProcessLogEvent>,

    /// This process's [Table].
    pub table: Table,

    /// This process's [MailboxStore].
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

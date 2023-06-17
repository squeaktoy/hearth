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

pub mod context;
pub mod factory;
pub mod local;
pub mod registry;
pub mod remote;
pub mod rpc;
pub mod store;

use local::LocalProcess;
use remote::RemoteProcess;
use store::{ProcessEntry, Signal};

/// The default [store::ProcessStoreTrait] implementation.
pub type ProcessStore = store::ProcessStore<AnyProcess>;

/// The default process registry using [ProcessStore].
pub type Registry = registry::Registry<ProcessStore>;

/// The default process factory using [ProcessStore].
pub type ProcessFactory = factory::ProcessFactory<ProcessStore>;

/// The default local process using [ProcessStore].
pub type Process = factory::Process<ProcessStore>;

#[derive(Default)]
pub struct AnyProcessData {
    pub local: <LocalProcess as ProcessEntry>::Data,
    pub remote: <RemoteProcess as ProcessEntry>::Data,
}

/// A process entry that can be either remote or local.
pub enum AnyProcess {
    Local(LocalProcess),
    Remote(RemoteProcess),
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
            AnyProcess::Remote(remote) => remote.on_insert(&data.remote, handle),
        }
    }

    fn on_signal(&self, data: &Self::Data, signal: Signal) -> Option<Signal> {
        match self {
            AnyProcess::Local(local) => local.on_signal(&data.local, signal),
            AnyProcess::Remote(remote) => remote.on_signal(&data.remote, signal),
        }
    }

    fn on_remove(&self, data: &Self::Data) {
        match self {
            AnyProcess::Local(local) => local.on_remove(&data.local),
            AnyProcess::Remote(remote) => remote.on_remove(&data.remote),
        }
    }
}

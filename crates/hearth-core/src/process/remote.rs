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

//! Networking code between a remote peer and the local process store.

use std::collections::HashMap;
use std::sync::Arc;

use hearth_rpc::caps::{LocalCapOperation, RemoteCapOperation, UnlinkReason};
use hearth_rpc::hearth_types::Flags;
use hearth_rpc::CapOperation;
use slab::Slab;
use tokio::sync::{mpsc, oneshot};
use tracing::warn;

use crate::process::store::Message;

use super::context::Capability;
use super::registry::Registry;
use super::store::{ProcessEntry, ProcessStoreTrait, Signal};

/// The [ProcessEntry::Data] for [RemoteProcess].
#[derive(Default)]
pub struct RemoteProcessData {}

/// A [ProcessEntry] implementation for remote processes.
pub struct RemoteProcess {
    /// The capability ID on this process's connection.
    cap_id: u32,

    /// The connection's outgoing signal mailbox.
    cap_signal_tx: mpsc::UnboundedSender<(u32, Signal)>,
}

impl ProcessEntry for RemoteProcess {
    type Data = RemoteProcessData;

    fn on_insert(&self, _data: &Self::Data, _handle: usize) {}

    fn on_signal(&self, _data: &Self::Data, signal: Signal) -> Option<Signal> {
        self.cap_signal_tx
            .send((self.cap_id, signal))
            .err()
            .map(|err| err.0)
            .map(|(_id, signal)| signal)
    }

    fn on_remove(&self, _data: &Self::Data) {}
}

pub type RequestCb<T> = Box<dyn FnOnce(T) + 'static>;

pub struct Connection<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    registry: Arc<Registry<Store>>,
    local_caps: Slab<Option<Capability>>,
    remote_caps: HashMap<u32, Capability>,
    list_services_reqs: Slab<RequestCb<Vec<String>>>,
    get_service_reqs: Slab<RequestCb<Option<Capability>>>,
    cap_signal_tx: mpsc::UnboundedSender<(u32, Signal)>,
    op_tx: mpsc::UnboundedSender<CapOperation>,
}

impl<Store: ProcessStoreTrait> Drop for Connection<Store> {
    fn drop(&mut self) {
        for cap in self.local_caps.drain().flatten() {
            cap.free(self.store.as_ref());
        }

        for (_, cap) in self.remote_caps.drain() {
            cap.free(self.store.as_ref());
        }
    }
}

impl<Store> Connection<Store>
where
    Store: ProcessStoreTrait + 'static,
    Store::Entry: From<RemoteProcess>,
{
    pub fn new(
        store: Arc<Store>,
        registry: Arc<Registry<Store>>,
        cap_signal_tx: mpsc::UnboundedSender<(u32, Signal)>,
        op_tx: mpsc::UnboundedSender<CapOperation>,
    ) -> Self {
        Self {
            store,
            registry,
            local_caps: Slab::new(),
            remote_caps: HashMap::new(),
            list_services_reqs: Slab::new(),
            get_service_reqs: Slab::new(),
            cap_signal_tx,
            op_tx,
        }
    }

    pub async fn list_services(&mut self) -> Vec<String> {
        let (req_tx, req) = oneshot::channel();

        let req_id = self.list_services_reqs.insert(Box::new(move |services| {
            let _ = req_tx.send(services);
        })) as u32;

        self.send_remote_op(RemoteCapOperation::ListServicesRequest { req_id });

        req.await.ok().unwrap_or_default()
    }

    pub async fn get_service(&mut self, name: String) -> Option<Capability> {
        let (req_tx, req) = oneshot::channel();

        let store = self.store.to_owned();
        let req_id = self.get_service_reqs.insert(Box::new(move |cap| {
            req_tx
                .send(cap)
                .err()
                .flatten()
                .map(|cap| cap.free(store.as_ref()));
        })) as u32;

        self.send_remote_op(RemoteCapOperation::GetServiceRequest { req_id, name });

        req.await.ok().flatten()
    }

    /// Revokes a local capability from the remote cap.
    pub fn revoke(&mut self, id: u32, reason: UnlinkReason) {
        let Some(cap) = self.local_caps.get_mut(id as usize) else { return };
        cap.take().map(|cap| {
            self.send_local_op(LocalCapOperation::RevokeCap { id, reason });
            cap.free(self.store.as_ref());
        });
    }

    pub fn on_op(&mut self, op: CapOperation) {
        match op {
            CapOperation::Local(op) => self.on_local_op(op),
            CapOperation::Remote(op) => self.on_remote_op(op),
        }
    }

    fn on_local_op(&mut self, op: LocalCapOperation) {
        use LocalCapOperation::*;
        match op {
            DeclareCap { id, flags } => {
                if self.remote_caps.contains_key(&id) {
                    warn!("peer attempted to re-declare a cap ID");
                    return;
                }

                let process = RemoteProcess {
                    cap_id: id,
                    cap_signal_tx: self.cap_signal_tx.clone(),
                };

                let handle = self.store.insert(process.into());
                let cap = Capability::new(handle, flags);
                self.remote_caps.insert(id, cap);
            }
            RevokeCap { id, reason: _ } => {
                let Some(cap) = self.remote_caps.remove(&id) else { return; };
                self.store.kill(cap.get_handle()); // TODO kill reason?
                cap.free(self.store.as_ref());
                self.send_remote_op(RemoteCapOperation::AcknowledgeRevocation { id });
            }
            ListServicesResponse { req_id, services } => {
                if let Some(cb) = self.list_services_reqs.try_remove(req_id as usize) {
                    cb(services);
                }
            }
            GetServiceResponse {
                req_id,
                service_cap,
            } => {
                if let Some(cb) = self.get_service_reqs.try_remove(req_id as usize) {
                    if let Some(service_cap) = service_cap {
                        if let Some(cap) = self
                            .remote_caps
                            .get(&service_cap)
                            .map(|cap| cap.clone(self.store.as_ref()))
                        {
                            cb(Some(cap));
                        }
                    } else {
                        cb(None);
                    }
                }
            }
        }
    }

    fn on_remote_op(&mut self, op: RemoteCapOperation) {
        use RemoteCapOperation::*;
        match op {
            AcknowledgeRevocation { id } => {
                if let Some(None) = self.local_caps.get(id as usize) {
                    self.local_caps.remove(id as usize);
                }
            }
            FreeCap { id } => {
                if let Some(cap) = self.local_caps.try_remove(id as usize).flatten() {
                    cap.free(self.store.as_ref());
                }
            }
            ListServicesRequest { req_id } => {
                self.send_local_op(LocalCapOperation::ListServicesResponse {
                    req_id,
                    services: self.registry.list(),
                });
            }
            GetServiceRequest { req_id, name } => {
                let service_cap = self.registry.get(name).map(|cap| self.add_local_cap(cap));
                self.send_local_op(LocalCapOperation::GetServiceResponse {
                    req_id,
                    service_cap,
                });
            }
            Send { id, data, caps } => {
                let Some(Some(cap)) = self.local_caps.get(id as usize) else {
                    return;
                };

                if !cap.get_flags().contains(Flags::SEND) {
                    warn!("peer attempted unpermitted send operation");
                    return;
                }

                let mut store_caps = Vec::with_capacity(caps.len());
                for cap_id in caps {
                    if let Some(cap) = self.remote_caps.get(&cap_id) {
                        store_caps.push(cap.clone(self.store.as_ref()));
                    } else {
                        warn!("peer transferred invalid cap ID");
                        return;
                    }
                }

                self.store.send(
                    cap.get_handle(),
                    Message {
                        data,
                        caps: store_caps,
                    },
                );
            }
            Kill { id } => {
                if let Some(Some(cap)) = self.local_caps.get(id as usize) {
                    if cap.get_flags().contains(Flags::KILL) {
                        self.store.kill(cap.get_handle());
                    } else {
                        warn!("peer attempted unpermitted kill operation");
                    }
                }
            }
        }
    }

    fn add_local_cap(&mut self, cap: Capability) -> u32 {
        let flags = cap.get_flags();
        let id = self.local_caps.insert(Some(cap)) as u32;
        self.send_local_op(LocalCapOperation::DeclareCap { id, flags });
        id
    }

    fn send_local_op(&self, op: LocalCapOperation) {
        let _ = self.op_tx.send(CapOperation::Local(op));
    }

    fn send_remote_op(&self, op: RemoteCapOperation) {
        let _ = self.op_tx.send(CapOperation::Remote(op));
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use crate::process::ProcessStore;

    /// Utility struct to test connections.
    struct ConnectionEnv {
        pub connection: Connection<ProcessStore>,
        pub signal_rx: mpsc::UnboundedReceiver<(u32, Signal)>,
        pub op_rx: mpsc::UnboundedReceiver<CapOperation>,
    }

    impl std::ops::Deref for ConnectionEnv {
        type Target = Connection<ProcessStore>;

        fn deref(&self) -> &Self::Target {
            &self.connection
        }
    }

    impl std::ops::DerefMut for ConnectionEnv {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.connection
        }
    }

    impl ConnectionEnv {
        pub fn new() -> Self {
            let store = Arc::new(ProcessStore::default());
            let registry = Arc::new(Registry::new(store.to_owned()));
            let (signal_tx, signal_rx) = mpsc::unbounded_channel();
            let (op_tx, op_rx) = mpsc::unbounded_channel();
            let connection = Connection::new(store, registry, signal_tx, op_tx);

            Self {
                connection,
                signal_rx,
                op_rx,
            }
        }

        /// Declares a mock remote capability within this connection.
        pub fn declare_mock_cap(&mut self, id: u32, flags: Flags) -> Capability {
            self.on_local_op(LocalCapOperation::DeclareCap { id, flags });
            let cap = self.remote_caps.get(&id).unwrap();
            cap.clone(self.store.as_ref())
        }
    }

    #[test]
    fn create_connection() {
        let _ = ConnectionEnv::new();
    }

    #[test]
    fn declare_cap() {
        let mut conn = ConnectionEnv::new();
        let cap = conn.declare_mock_cap(0, Flags::empty());
        cap.free(conn.store.as_ref());
    }

    #[tokio::test]
    async fn local_signal_send() {
        let mut conn = ConnectionEnv::new();
        let cap = conn.declare_mock_cap(0, Flags::SEND);

        let msg = Message {
            data: b"Hello, world!".to_vec(),
            caps: vec![],
        };

        let send_msg = msg.clone(conn.store.as_ref());
        conn.store.send(cap.get_handle(), send_msg);
        cap.free(conn.store.as_ref());

        assert_eq!(
            conn.signal_rx.try_recv().unwrap(),
            (0, Signal::Message(msg))
        );
    }
}

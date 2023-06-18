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
//!
//! This networking code is loosely based on [CapTP](http://www.erights.org/elib/distrib/captp/index.html).
//! It is highly recommended to read CapTP's documentation to become familiar
//! with the core concepts of Hearth's capability networking.

use std::collections::HashMap;
use std::sync::Arc;

use hearth_rpc::caps::{LocalCapOperation, RemoteCapOperation, UnlinkReason};
use hearth_rpc::hearth_types::Flags;
use hearth_rpc::CapOperation;
use parking_lot::Mutex;
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

/// A connection's imports table. Maps capabilities exported by the remote vat
/// to local [Capability] objects.
pub struct ImportsTable<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    imports: HashMap<u32, Capability>,
}

impl<Store: ProcessStoreTrait> Drop for ImportsTable<Store> {
    fn drop(&mut self) {
        for (_, cap) in self.imports.drain() {
            cap.free(self.store.as_ref());
        }
    }
}

impl<Store: ProcessStoreTrait> ImportsTable<Store> {
    /// Creates an empty imports table.
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            imports: HashMap::new(),
        }
    }

    /// Inserts an exported capability into this table.
    pub fn insert(&mut self, id: u32, cap: Capability) {
        if let Some(old_cap) = self.imports.insert(id, cap) {
            // TODO better overwriting behavior?
            warn!("export ID {} overwrites existing cap", id);
            old_cap.free(self.store.as_ref());
        }
    }

    /// Maps an import ID into a local capability, if valid.
    pub fn get(&self, id: u32) -> Option<Capability> {
        self.imports
            .get(&id)
            .map(|cap| cap.clone(self.store.as_ref()))
    }

    /// Removes an imported capability from this store.
    ///
    /// Returns true if the capability was removed, false if the ID was invalid.
    pub fn remove(&mut self, id: u32) -> bool {
        if let Some(old_cap) = self.imports.remove(&id) {
            self.store.kill(old_cap.get_handle());
            old_cap.free(self.store.as_ref());
            true
        } else {
            false
        }
    }
}

/// A connection's exports table. Manages exported IDs for local capabilities.
pub struct ExportsTable<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    exports: Slab<Option<Capability>>,
}

impl<Store: ProcessStoreTrait> Drop for ExportsTable<Store> {
    fn drop(&mut self) {
        for cap in self.exports.drain().flatten() {
            cap.free(self.store.as_ref());
        }
    }
}

impl<Store: ProcessStoreTrait> ExportsTable<Store> {
    /// Creates an empty exports table.
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            exports: Slab::new(),
        }
    }

    /// Adds an exported capability into this table.
    pub fn insert(&mut self, cap: Capability) -> u32 {
        self.exports.insert(Some(cap)) as u32
    }

    /// Revokes a capability, making it invalid without freeing its ID.
    ///
    /// Returns true if revoked, false if the capability is already revoked.
    pub fn revoke(&mut self, id: u32) -> bool {
        let slot = self.exports.get_mut(id as usize);
        let Some(cap) = slot else { return false };
        let Some(old_cap) = cap.take() else { return false };
        old_cap.free(self.store.as_ref());
        true
    }

    /// Frees a capability. This should only be done after [Self::revoke] has
    /// been acknowledged.
    pub fn free(&mut self, id: u32) {
        let slot = self.exports.try_remove(id as usize);
        let Some(Some(old_cap)) = slot else { return };
        old_cap.free(self.store.as_ref());
    }

    /// Sends a locally-mapped message to an exported capability. No-ops if
    /// the ID is invalid or the operation is unpermitted.
    pub fn send(&self, id: u32, message: Message) {
        if let Some(Some(cap)) = self.exports.get(id as usize) {
            if cap.get_flags().contains(Flags::SEND) {
                self.store.send(cap.get_handle(), message);
                return;
            } else {
                warn!("exported capability send operation is unpermitted");
            }
        }

        message.free(self.store.as_ref());
    }

    /// Kills an exported capability. No-ops if the ID is invalid or the
    /// operation is unpermitted.
    pub fn kill(&self, id: u32) {
        let slot = self.exports.get(id as usize);
        let Some(Some(cap)) = slot else { return };

        if cap.get_flags().contains(Flags::KILL) {
            self.store.kill(cap.get_handle());
        } else {
            warn!("exported capability kill operation is unpermitted");
        }
    }
}

pub type RequestCb<T> = Box<dyn FnOnce(T) + Send + 'static>;

pub struct Connection<Store: ProcessStoreTrait> {
    store: Arc<Store>,
    registry: Arc<Registry<Store>>,
    imports: ImportsTable<Store>,
    exports: ExportsTable<Store>,
    list_services_reqs: Slab<RequestCb<Vec<String>>>,
    get_service_reqs: Slab<RequestCb<Option<Capability>>>,
    cap_signal_tx: mpsc::UnboundedSender<(u32, Signal)>,
    op_tx: mpsc::UnboundedSender<CapOperation>,
}

impl<Store> Connection<Store>
where
    Store: ProcessStoreTrait + Send + Sync + 'static,
    Store::Entry: From<RemoteProcess>,
{
    /// Creates a new connection.
    ///
    /// `op_rx` is the channel receiver used to receive incoming [CapOperation]
    /// messages on this connection. `op_rx` is the channel sender used to send
    /// outgoing [CapOperation]s.
    pub fn new(
        store: Arc<Store>,
        registry: Arc<Registry<Store>>,
        op_rx: mpsc::UnboundedReceiver<CapOperation>,
        op_tx: mpsc::UnboundedSender<CapOperation>,
    ) -> Arc<Mutex<Self>> {
        let (signal_tx, signal_rx) = mpsc::unbounded_channel();
        let conn = Self::new_unspawned(store, registry, signal_tx, op_tx);
        let conn = Arc::new(Mutex::new(conn));
        Self::spawn_signal_rx(conn.to_owned(), signal_rx);
        Self::spawn_op_rx(conn.to_owned(), op_rx);
        conn
    }

    /// Internal constructor used to create a connection without spawning
    /// threads to pump the signal and operation channels.
    pub(crate) fn new_unspawned(
        store: Arc<Store>,
        registry: Arc<Registry<Store>>,
        cap_signal_tx: mpsc::UnboundedSender<(u32, Signal)>,
        op_tx: mpsc::UnboundedSender<CapOperation>,
    ) -> Self {
        Self {
            store: store.to_owned(),
            registry,
            imports: ImportsTable::new(store.to_owned()),
            exports: ExportsTable::new(store.to_owned()),
            list_services_reqs: Slab::new(),
            get_service_reqs: Slab::new(),
            cap_signal_tx,
            op_tx,
        }
    }

    /// Spawns a thread that processes received signals from the given channel.
    pub fn spawn_signal_rx(
        conn: Arc<Mutex<Self>>,
        mut signal_rx: mpsc::UnboundedReceiver<(u32, Signal)>,
    ) {
        tokio::spawn(async move {
            while let Some((id, signal)) = signal_rx.recv().await {
                let mut conn = conn.lock();
                match signal {
                    Signal::Kill => {
                        conn.send_remote_op(RemoteCapOperation::Kill { id });
                    }
                    Signal::Unlink { subject } => {
                        // TODO networked unlinking?
                        conn.store.dec_ref(subject);
                    }
                    Signal::Message(Message { data, caps }) => {
                        let caps = caps.into_iter().map(|cap| conn.export(cap)).collect();
                        conn.send_remote_op(RemoteCapOperation::Send { id, data, caps });
                    }
                }
            }
        });
    }

    /// Spawns a thread that will process received operations from the given
    /// channel.
    pub fn spawn_op_rx(conn: Arc<Mutex<Self>>, mut op_rx: mpsc::UnboundedReceiver<CapOperation>) {
        tokio::spawn(async move {
            while let Some(op) = op_rx.recv().await {
                conn.lock().on_op(op);
            }
        });
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

    /// Exports a capability through this connection.
    pub fn export(&mut self, cap: Capability) -> u32 {
        let flags = cap.get_flags();
        let id = self.exports.insert(cap);
        self.send_local_op(LocalCapOperation::DeclareCap { id, flags });
        id
    }

    /// Revokes a local capability from the connection.
    pub fn revoke(&mut self, id: u32, reason: UnlinkReason) {
        if self.exports.revoke(id) {
            self.send_local_op(LocalCapOperation::RevokeCap { id, reason });
        }
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
                let process = RemoteProcess {
                    cap_id: id,
                    cap_signal_tx: self.cap_signal_tx.clone(),
                };

                let handle = self.store.insert(process.into());
                let cap = Capability::new(handle, flags);
                self.imports.insert(id, cap);
            }
            RevokeCap { id, reason: _ } => {
                if self.imports.remove(id) {
                    self.send_remote_op(RemoteCapOperation::AcknowledgeRevocation { id });
                }
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
                            .imports
                            .get(service_cap)
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
                self.exports.free(id);
            }
            FreeCap { id } => {
                self.exports.free(id);
            }
            ListServicesRequest { req_id } => {
                self.send_local_op(LocalCapOperation::ListServicesResponse {
                    req_id,
                    services: self.registry.list(),
                });
            }
            GetServiceRequest { req_id, name } => {
                let service_cap = self.registry.get(name).map(|cap| self.export(cap));
                self.send_local_op(LocalCapOperation::GetServiceResponse {
                    req_id,
                    service_cap,
                });
            }
            Send { id, data, caps } => {
                let mut store_caps = Vec::with_capacity(caps.len());
                for cap_id in caps {
                    if let Some(cap) = self.imports.get(cap_id) {
                        store_caps.push(cap);
                    } else {
                        warn!("peer transferred invalid cap ID");
                        return;
                    }
                }

                self.exports.send(
                    id,
                    Message {
                        data,
                        caps: store_caps,
                    },
                );
            }
            Kill { id } => {
                self.exports.kill(id);
            }
        }
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

    impl Connection<ProcessStore> {
        /// Declares a mock remote capability within this connection.
        pub fn declare_mock_cap(&mut self, id: u32, flags: Flags) -> Capability {
            self.on_local_op(LocalCapOperation::DeclareCap { id, flags });
            self.imports.get(id).unwrap()
        }
    }

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
        pub fn new_unspawned() -> Self {
            let store = Arc::new(ProcessStore::default());
            let registry = Arc::new(Registry::new(store.to_owned()));
            let (signal_tx, signal_rx) = mpsc::unbounded_channel();
            let (op_tx, op_rx) = mpsc::unbounded_channel();
            let connection = Connection::new_unspawned(store, registry, signal_tx, op_tx);

            Self {
                connection,
                signal_rx,
                op_rx,
            }
        }

        pub fn spawn(
            self,
        ) -> (
            Arc<Mutex<Connection<ProcessStore>>>,
            mpsc::UnboundedReceiver<CapOperation>,
        ) {
            let conn = Arc::new(Mutex::new(self.connection));
            Connection::spawn_signal_rx(conn.to_owned(), self.signal_rx);
            (conn, self.op_rx)
        }
    }

    #[test]
    fn create_connection() {
        let _ = ConnectionEnv::new_unspawned();
    }

    #[test]
    fn declare_cap() {
        let mut conn = ConnectionEnv::new_unspawned();
        let cap = conn.declare_mock_cap(0, Flags::empty());
        cap.free(conn.store.as_ref());
    }

    #[tokio::test]
    async fn signal_send() {
        let mut conn = ConnectionEnv::new_unspawned();
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

    #[tokio::test]
    async fn kill() {
        let conn = ConnectionEnv::new_unspawned();
        let (conn, mut op_rx) = conn.spawn();
        let cap = conn.lock().declare_mock_cap(0, Flags::KILL);

        let store = conn.lock().store.to_owned();
        store.kill(cap.get_handle());
        cap.free(store.as_ref());

        // let the connection thread handle the new message
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(
            op_rx.try_recv().unwrap(),
            CapOperation::Remote(RemoteCapOperation::Kill { id: 0 }),
        );
    }

    #[tokio::test]
    async fn send() {
        let conn = ConnectionEnv::new_unspawned();
        let (conn, mut op_rx) = conn.spawn();
        let cap = conn.lock().declare_mock_cap(0, Flags::SEND);

        let data = b"Hello, world!".to_vec();
        let msg = Message {
            data: data.clone(),
            caps: vec![],
        };

        let store = conn.lock().store.to_owned();
        let send_msg = msg.clone(store.as_ref());
        store.send(cap.get_handle(), send_msg);
        cap.free(store.as_ref());

        // let the connection thread handle the new message
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(
            op_rx.try_recv().unwrap(),
            CapOperation::Remote(RemoteCapOperation::Send {
                id: 0,
                data,
                caps: vec![]
            }),
        );
    }

    pub mod contexts {
        use super::*;

        use crate::process::context::{ContextMessage, ContextSignal};
        use crate::process::{Process, ProcessFactory};
        use hearth_rpc::ProcessInfo;

        pub async fn make_connected() -> ((Process, usize), (Process, usize)) {
            let init_side = |peer_id| {
                let conn = ConnectionEnv::new_unspawned();
                let factory = ProcessFactory::new(
                    conn.store.to_owned(),
                    conn.registry.to_owned(),
                    hearth_rpc::hearth_types::PeerId(peer_id),
                );
                let (conn, conn_rx) = conn.spawn();
                let ctx = factory.spawn(ProcessInfo {}, Flags::all());
                (conn, conn_rx, ctx)
            };

            let (a, a_rx, a_ctx) = init_side(0);
            let (b, b_rx, b_ctx) = init_side(1);

            Connection::spawn_op_rx(a.to_owned(), b_rx);
            Connection::spawn_op_rx(b.to_owned(), a_rx);

            let mut a = a.lock();
            let a_cap = a.export(a_ctx.get_self_capability());

            let mut b = b.lock();
            let b_cap = b.export(b_ctx.get_self_capability());

            a.send_remote_op(RemoteCapOperation::Send {
                id: b_cap,
                data: vec![],
                caps: vec![a_cap],
            });

            b.send_remote_op(RemoteCapOperation::Send {
                id: a_cap,
                data: vec![],
                caps: vec![b_cap],
            });

            drop(a);
            drop(b);

            async fn recv_cap(mut ctx: Process) -> (Process, usize) {
                let signal = ctx.recv().await.unwrap();
                let cap = match signal {
                    crate::process::context::ContextSignal::Unlink { .. } => {
                        panic!("expected message, got unlink")
                    }
                    crate::process::context::ContextSignal::Message(ContextMessage {
                        data,
                        mut caps,
                    }) => {
                        assert!(data.is_empty());
                        assert_eq!(caps.len(), 1);
                        caps.remove(0)
                    }
                };

                (ctx, cap)
            }

            (recv_cap(a_ctx).await, recv_cap(b_ctx).await)
        }

        #[tokio::test]
        async fn create_connected_contexts() {
            let _ = make_connected().await;
        }

        #[tokio::test]
        async fn send_data() {
            let ((a_ctx, b_cap), (mut b_ctx, _a_cap)) = make_connected().await;

            let data = b"Hello, world!".to_vec();

            a_ctx
                .send(
                    b_cap,
                    ContextMessage {
                        data: data.clone(),
                        caps: vec![],
                    },
                )
                .unwrap();

            assert_eq!(
                b_ctx.recv().await.unwrap(),
                ContextSignal::Message(ContextMessage { data, caps: vec![] })
            );
        }
    }
}

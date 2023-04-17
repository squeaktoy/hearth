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

use super::*;
use remoc::rtc::ServerShared;
use remoc::{
    robs::{hash_map::ObservableHashMap, list::ObservableList},
    rtc::async_trait,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct MockProcessStore {
    services: RwLock<ObservableHashMap<String, LocalProcessId>>,
    processes: ObservableHashMap<LocalProcessId, ProcessStatus>,
    mock_processes: HashMap<LocalProcessId, ProcessApiClient>,
}

#[async_trait]
impl ProcessStore for MockProcessStore {
    async fn print_hello_world(&self) -> CallResult<()> {
        Ok(())
    }

    async fn find_process(&self, pid: LocalProcessId) -> ResourceResult<ProcessApiClient> {
        match self.mock_processes.get(&pid) {
            None => Err(ResourceError::Unavailable),
            Some(api) => Ok(api.clone()),
        }
    }

    async fn register_service(&self, pid: LocalProcessId, name: String) -> ResourceResult<()> {
        if !self.processes.contains_key(&pid) {
            return Err(ResourceError::Unavailable);
        }
        let mut services = self.services.write().await;
        if services.contains_key(&name) {
            return Err(ResourceError::BadParams);
        }
        services.insert(name, pid);
        Ok(())
    }

    async fn deregister_service(&self, name: String) -> ResourceResult<()> {
        match self.services.write().await.remove(&name) {
            None => Err(ResourceError::Unavailable),
            _ => Ok(()),
        }
    }

    async fn follow_process_list(
        &self,
    ) -> CallResult<HashMapSubscription<LocalProcessId, ProcessStatus>> {
        Ok(self.processes.subscribe(128))
    }

    async fn follow_service_list(&self) -> CallResult<HashMapSubscription<String, LocalProcessId>> {
        Ok(self.services.read().await.subscribe(128))
    }
}

impl MockProcessStore {
    pub fn new() -> Self {
        let mut processes = ObservableHashMap::new();
        let mut mock_processes = HashMap::new();
        let test_pid = LocalProcessId(0);
        let (_, warning_num) = watch::channel(0);
        let (_, error_num) = watch::channel(0);
        let (_, log_num) = watch::channel(0);
        let info = ProcessInfo {};
        let test_info = ProcessStatus {
            warning_num,
            error_num,
            log_num,
            info,
        };
        let test_process = MockProcessApi::new();
        processes.insert(test_pid, test_info);
        let (process_server, process_client) =
            ProcessApiServerShared::<_, remoc::codec::Default>::new(Arc::new(test_process), 1024);
        tokio::spawn(async move {
            process_server.serve(true).await;
        });
        mock_processes.insert(test_pid, process_client);
        Self {
            services: Default::default(),
            processes,
            mock_processes,
        }
    }
}

#[derive(Debug)]
pub struct MockProcessFactory {}

#[async_trait]
impl ProcessFactory for MockProcessFactory {
    async fn spawn(&self, _process: ProcessBase) -> CallResult<ProcessOffer> {
        let (outgoing, _) = mpsc::channel(1024);
        Ok(ProcessOffer {
            outgoing,
            pid: LocalProcessId(0),
        })
    }
}

#[derive(Debug)]
pub struct MockProcessApi {
    log: RwLock<ObservableList<ProcessLogEvent>>,
}

#[async_trait]
impl ProcessApi for MockProcessApi {
    async fn is_alive(&self) -> CallResult<bool> {
        Ok(true)
    }

    async fn kill(&self) -> CallResult<()> {
        Ok(())
    }

    async fn follow_log(&self) -> CallResult<ListSubscription<ProcessLogEvent>> {
        Ok(self.log.read().await.subscribe())
    }
}

impl MockProcessApi {
    pub fn new() -> Self {
        Self {
            log: RwLock::new(
                vec![
                    ProcessLogEvent {
                        level: ProcessLogLevel::Info,
                        module: String::from("init"),
                        content: String::from(
                            "This is an info level log message generated on process initialization",
                        ),
                    },
                    ProcessLogEvent {
                        level: ProcessLogLevel::Warning,
                        module: String::from("init"),
                        content: String::from("This is a mock process"),
                    },
                    ProcessLogEvent {
                        level: ProcessLogLevel::Trace,
                        module: String::from("tracer from overwatch"),
                        content: String::from("low level thing you cant understand"),
                    },
                    ProcessLogEvent {
                        level: ProcessLogLevel::Debug,
                        module: String::from("spider"),
                        content: String::from("The spider has been de-bugged :("),
                    },
                    ProcessLogEvent {
                        level: ProcessLogLevel::Error,
                        module: String::from("awwww fuck"),
                        content: String::from("oi can belie ya don dis"),
                    },
                ]
                .into(),
            ),
        }
    }
}

#[derive(Debug)]
pub struct MockPeerApi {
    peer_info: PeerInfo,
    process_store: ProcessStoreClient,
}

#[async_trait]
impl PeerApi for MockPeerApi {
    async fn get_info(&self) -> CallResult<PeerInfo> {
        Ok(self.peer_info.clone())
    }

    async fn get_process_store(&self) -> CallResult<ProcessStoreClient> {
        Ok(self.process_store.clone())
    }

    async fn get_lump_store(&self) -> CallResult<LumpStoreClient> {
        Err(CallError::RemoteForward)
    }
}

impl MockPeerApi {
    pub fn new() -> Self {
        let test_store = MockProcessStore::new();
        let (store_server, process_store) =
            ProcessStoreServerShared::<_, remoc::codec::Default>::new(Arc::new(test_store), 1024);
        tokio::spawn(async move {
            store_server.serve(true).await;
        });
        MockPeerApi {
            peer_info: PeerInfo {
                nickname: Some("New peer".into()),
            },
            process_store,
        }
    }
}

#[derive(Debug)]
pub struct MockPeerProvider {
    peers: HashMap<PeerId, PeerApiClient>,
    peer_info: ObservableHashMap<PeerId, PeerInfo>,
}

#[async_trait]
impl PeerProvider for MockPeerProvider {
    async fn find_peer(&self, id: PeerId) -> ResourceResult<PeerApiClient> {
        match self.peers.get(&id) {
            None => Err(ResourceError::Unavailable),
            Some(peer) => Ok(peer.clone()),
        }
    }

    async fn follow_peer_list(&self) -> CallResult<HashMapSubscription<PeerId, PeerInfo>> {
        Ok(self.peer_info.subscribe(128))
    }
}

impl MockPeerProvider {
    pub fn new() -> Self {
        let mut peer_info = ObservableHashMap::new();
        let mut peers = HashMap::new();
        let test_pid = PeerId(0);
        let test_info = PeerInfo {
            nickname: { Some("Silly Peer".into()) },
        };
        let test_process = MockPeerApi::new();
        peer_info.insert(test_pid, test_info);
        let (peer_server, peer_client) =
            PeerApiServerShared::<_, remoc::codec::Default>::new(Arc::new(test_process), 1024);
        tokio::spawn(async move {
            peer_server.serve(true).await;
        });
        peers.insert(test_pid, peer_client);
        Self { peers, peer_info }
    }
}

#[derive(Debug)]
pub struct MockLumpStore {}

#[async_trait]
impl LumpStore for MockLumpStore {
    async fn upload_lump(&self, _id: Option<LumpId>, _data: LazyBlob) -> ResourceResult<LumpId> {
        Err(ResourceError::BadParams)
    }

    /// Downloads a lump from this store.
    async fn download_lump(&self, _id: LumpId) -> ResourceResult<LazyBlob> {
        Err(ResourceError::Unavailable)
    }
}

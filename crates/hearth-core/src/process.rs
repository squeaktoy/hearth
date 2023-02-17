use std::collections::HashMap;
use std::sync::{Arc, Weak};

use hearth_rpc::remoc::robs::hash_map::HashMapSubscription;
use hearth_rpc::remoc::rtc::ServerShared;
use hearth_rpc::*;
use hearth_types::*;
use remoc::robs::hash_map::ObservableHashMap;
use remoc::robs::list::{ListSubscription, ObservableList, ObservableListDistributor};
use remoc::rtc::async_trait;
use tokio::sync::{mpsc, watch, RwLock};
use tracing::{debug, error, info, trace};

use crate::runtime::Runtime;

#[async_trait]
pub trait Process: Send + Sync + 'static {
    async fn run(&mut self, ctx: ProcessContext);
}

#[derive(Clone, Debug)]
pub struct Message {
    /// The ID of the process that sent this message.
    ///
    /// This may be used as a return address for reply messages.
    pub sender: ProcessId,

    /// The message's data.
    pub data: Vec<u8>,
}

/// The full set of resources provided for a single process.
///
/// Process implementations may use this struct to perform all of their duties
/// as a process, including sending and receiving messages, accessing the
/// full runtime, and logging events to this process's log.
pub struct ProcessContext {
    /// The ID of this process.
    pid: ProcessId,

    /// The runtime that this process is a part of.
    runtime: Arc<Runtime>,

    /// A queue of all messages sent to this process.
    mailbox: mpsc::Receiver<Message>,

    /// True when this process is not dead.
    is_alive: watch::Receiver<bool>,

    /// Sender to set [is_alive] itself.
    is_alive_tx: Arc<watch::Sender<bool>>,

    /// Channel to send IDs of killed processes to.
    on_kill: mpsc::UnboundedSender<LocalProcessId>,

    /// Observable log for this process's log events.
    log: ObservableList<ProcessLogEvent>,
}

impl Drop for ProcessContext {
    fn drop(&mut self) {
        // kill this process if it hasn't been already
        if self.is_alive() {
            self.kill();
        }
    }
}

impl ProcessContext {
    /// Gets the [ProcessId] for this process.
    pub fn get_pid(&self) -> ProcessId {
        self.pid
    }

    /// Returns a reference to the runtime this process is a part of.
    pub fn get_runtime(&self) -> &Arc<Runtime> {
        &self.runtime
    }

    /// Returns true when this process is still alive.
    pub fn is_alive(&self) -> bool {
        *self.is_alive.borrow()
    }

    /// Waits for this process to complete.
    ///
    /// Can be combined with `tokio::select` to wait on another future or quit
    /// when this process is killed. Useful for signalling async processes to
    /// exit their event loop.
    pub async fn join(&mut self) {
        while self.is_alive() {
            let _ = self.is_alive.changed().await;
        }
    }

    /// Receives a single message to this process.
    ///
    /// Returns `None` if this process is dead.
    pub async fn recv(&mut self) -> Option<Message> {
        self.mailbox.recv().await
    }

    /// Kills this process.
    ///
    /// Does nothing if it's already been killed.
    pub fn kill(&mut self) {
        if self.is_alive_tx.send_replace(false) {
            let (_peer, local_pid) = self.pid.split();
            let _ = self.on_kill.send(local_pid); // ignore result; not responsible for killing if the receiver's store is unavailable
        }
    }

    /// Adds a log event to this process's log.
    pub fn log(&mut self, event: ProcessLogEvent) {
        self.log.push(event);
    }
}

#[derive(Clone)]
struct ProcessWrapper {
    mailbox: mpsc::Sender<Message>,
    log_distributor: ObservableListDistributor<ProcessLogEvent>,
    is_alive_tx: Arc<watch::Sender<bool>>,
}

struct ProcessApiImpl {
    pid: LocalProcessId,
    log_distributor: ObservableListDistributor<ProcessLogEvent>,
    is_alive_tx: Arc<watch::Sender<bool>>,
    on_kill_tx: mpsc::UnboundedSender<LocalProcessId>,
}

#[async_trait]
impl ProcessApi for ProcessApiImpl {
    async fn is_alive(&self) -> CallResult<bool> {
        Ok(*self.is_alive_tx.borrow())
    }

    async fn kill(&self) -> CallResult<()> {
        if self.is_alive_tx.send_replace(false) {
            let _ = self.on_kill_tx.send(self.pid);
        }

        Ok(())
    }

    async fn follow_log(&self) -> CallResult<ListSubscription<ProcessLogEvent>> {
        Ok(self.log_distributor.subscribe())
    }
}

struct ProcessStoreInner {
    services: ObservableHashMap<String, LocalProcessId>,
    processes: HashMap<LocalProcessId, ProcessWrapper>,
    process_infos: ObservableHashMap<LocalProcessId, ProcessInfo>,
}

impl ProcessStoreInner {
    fn new() -> Self {
        Self {
            services: Default::default(),
            processes: Default::default(),
            process_infos: Default::default(),
        }
    }
}

pub struct ProcessStoreImpl {
    inner: RwLock<ProcessStoreInner>,
    on_kill_tx: mpsc::UnboundedSender<LocalProcessId>,
}

impl ProcessStoreImpl {
    pub fn new() -> Arc<Self> {
        let (on_kill_tx, on_kill_rx) = mpsc::unbounded_channel();

        let store = Arc::new(Self {
            inner: RwLock::new(ProcessStoreInner::new()),
            on_kill_tx,
        });

        Self::listen_for_kills(Arc::downgrade(&store), on_kill_rx);

        store
    }

    fn listen_for_kills(store: Weak<Self>, mut on_kill: mpsc::UnboundedReceiver<LocalProcessId>) {
        debug!("Spawning on_kill listener thread");
        tokio::spawn(async move {
            loop {
                trace!("Listening for on_kill message");

                let pid = match on_kill.recv().await {
                    Some(pid) => pid,
                    None => {
                        debug!("All on_kill senders closed; exiting");
                        break;
                    }
                };

                trace!("Removing {:?} from store", pid);

                if let Some(store) = store.upgrade() {
                    let mut inner = store.inner.write().await;
                    inner.process_infos.remove(&pid);

                    // if there is actually a process with this ID, notify its context that it's dead
                    if let Some(wrapper) = inner.processes.remove(&pid) {
                        wrapper.is_alive_tx.send(false).unwrap();
                    } else {
                        // there is a chance of double-removal race conditions; that's a bug
                        error!("Attempted to kill dead PID {}", pid.0);
                    }
                } else {
                    debug!("All process store references dropped; exiting");
                    break;
                }
            }
        });
    }
}

#[async_trait]
impl ProcessStore for ProcessStoreImpl {
    async fn print_hello_world(&self) -> CallResult<()> {
        info!("Hello, world!");
        Ok(())
    }

    async fn find_process(&self, pid: LocalProcessId) -> ResourceResult<ProcessApiClient> {
        let store = self.inner.read().await;
        match store.processes.get(&pid).cloned() {
            None => Err(ResourceError::Unavailable),
            Some(wrapper) => {
                let api = Arc::new(ProcessApiImpl {
                    pid,
                    log_distributor: wrapper.log_distributor,
                    is_alive_tx: wrapper.is_alive_tx,
                    on_kill_tx: self.on_kill_tx.clone(),
                });

                let (server, client) =
                    ProcessApiServerShared::<_, remoc::codec::Default>::new(api, 128);

                tokio::spawn(async move {
                    server.serve(true).await;
                });

                Ok(client)
            }
        }
    }

    async fn register_service(&self, pid: LocalProcessId, name: String) -> ResourceResult<()> {
        debug!("Registering service '{}' to {:?}", name, pid);

        let mut store = self.inner.write().await;
        if !store.processes.contains_key(&pid) {
            debug!("Invalid local process ID");
            Err(ResourceError::Unavailable)
        } else if store.services.contains_key(&name) {
            debug!("Service name is taken");
            Err(ResourceError::BadParams)
        } else {
            store.services.insert(name, pid);
            Ok(())
        }
    }

    async fn deregister_service(&self, name: String) -> ResourceResult<()> {
        debug!("Deregistering service '{}'", name);
        if let None = self.inner.write().await.services.remove(&name) {
            Err(ResourceError::Unavailable)
        } else {
            Ok(())
        }
    }

    async fn follow_process_list(
        &self,
    ) -> CallResult<HashMapSubscription<LocalProcessId, ProcessInfo>> {
        Ok(self.inner.read().await.process_infos.subscribe(1024))
    }

    async fn follow_service_list(&self) -> CallResult<HashMapSubscription<String, LocalProcessId>> {
        Ok(self.inner.read().await.services.subscribe(1024))
    }
}

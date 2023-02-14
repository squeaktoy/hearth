use std::sync::Arc;

use hearth_rpc::hearth_types::*;
use hearth_rpc::remoc::rtc::async_trait;

use crate::runtime::Runtime;

#[async_trait]
pub trait Process: Send + Sync + 'static {
    async fn on_message(&self, from: ProcessId, data: Vec<u8>);
}

pub struct ProcessContext {
    /// The ID of this process.
    pub this_pid: ProcessId,

    /// The runtime that this process is a part of.
    pub runtime: Arc<Runtime>,
}

impl ProcessContext {
    pub fn send_message(&self, dst: ProcessId, data: Vec<u8>) {
        self.runtime.send_message(self.this_pid, dst, data);
    }
}

pub use hearth_types::*;

/// Fetches the process ID of the current process.
pub fn this_pid() -> ProcessId {
    let pid = unsafe { abi::this_pid() };
    ProcessId(pid)
}

/// Looks up the process ID of a peer's service by name.
pub fn service_lookup(peer: PeerId, name: &str) -> Option<ProcessId> {
    let bytes = name.as_bytes();
    let pid = unsafe { abi::service_lookup(peer.0, bytes.as_ptr() as u32, bytes.len() as u32) };
    if pid == u64::MAX {
        None
    } else {
        Some(ProcessId(pid))
    }
}

/// Registers a process as a service on its peer.
pub fn service_register(pid: ProcessId, name: &str) {
    let bytes = name.as_bytes();
    unsafe { abi::service_register(pid.0, bytes.as_ptr() as u32, bytes.len() as u32) }
}

/// Deregisters a peer's service.
pub fn service_deregister(peer: PeerId, name: &str) {
    let bytes = name.as_bytes();
    unsafe { abi::service_deregister(peer.0, bytes.as_ptr() as u32, bytes.len() as u32) }
}

/// Kills a process.
pub fn kill(pid: ProcessId) {
    unsafe { abi::kill(pid.0) }
}

/// Sends a message to another process.
pub fn send(pid: ProcessId, data: &[u8]) {
    unsafe { abi::send(pid.0, data.as_ptr() as u32, data.len() as u32) }
}

/// Blocks until a message has been received.
pub fn recv() -> Message {
    let msg = unsafe { abi::recv() };
    Message(msg)
}

/// Blocks until a message is received or the given timeout (in microseconds)
/// expires.
///
/// Setting the timeout to 0 skips any blocking and in effect polls the message
/// queue for a new message.
pub fn recv_timeout(timeout_us: u64) -> Message {
    let msg = unsafe { abi::recv_timeout(timeout_us) };
    Message(msg)
}

/// A message that has been received from another process.
pub struct Message(u32);

impl Drop for Message {
    fn drop(&mut self) {
        unsafe { abi::message_free(self.0) }
    }
}

impl Message {
    /// Gets the ID of the process that sent this message.
    pub fn get_sender(&self) -> ProcessId {
        let pid = unsafe { abi::message_get_sender(self.0) };
        ProcessId(pid)
    }

    /// Reads out the message data into an owning byte vector.
    pub fn get_data(&self) -> Vec<u8> {
        #[allow(clippy::uninit_vec)]
        unsafe {
            let len = abi::message_get_len(self.0) as usize;
            let mut data = Vec::with_capacity(len);
            data.set_len(len);
            abi::message_get_data(self.0, data.as_ptr() as u32);
            data
        }
    }
}

mod abi {
    #[link(wasm_import_module = "cognito")]
    extern "C" {
        pub fn this_pid() -> u64;
        pub fn service_lookup(peer: u32, name_ptr: u32, name_len: u32) -> u64;
        pub fn service_register(pid: u64, name_ptr: u32, name_len: u32);
        pub fn service_deregister(peer: u32, name_ptr: u32, name_len: u32);
        pub fn kill(pid: u64);
        pub fn send(pid: u64, ptr: u32, len: u32);
        pub fn recv() -> u32;
        pub fn recv_timeout(timeout_us: u64) -> u32;
        pub fn message_get_sender(msg: u32) -> u64;
        pub fn message_get_len(msg: u32) -> u32;
        pub fn message_get_data(msg: u32, ptr: u32);
        pub fn message_free(msg: u32);
    }
}

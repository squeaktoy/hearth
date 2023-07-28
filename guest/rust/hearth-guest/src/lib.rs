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

use serde::{Deserialize, Serialize};

pub use hearth_types::*;

/// Internal helper function to turn a string into a pointer and length.
fn abi_string(str: &str) -> (u32, u32) {
    let bytes = str.as_bytes();
    let ptr = bytes.as_ptr() as u32;
    let len = bytes.len() as u32;
    (ptr, len)
}

/// Fetches the lump ID of the module used to spawn the current process.
pub fn this_lump() -> LumpId {
    let mut id = LumpId(Default::default());
    unsafe { abi::lump::this_lump(&mut id as *const LumpId as u32) }
    id
}

/// The process currently executing.
pub static SELF: Process = Process(0);

lazy_static::lazy_static! {
    /// A lazily-initialized handle to the WebAssembly spawner service.
    pub static ref WASM_SPAWNER: Process = {
        Process::get_service("hearth.cognito.WasmProcessSpawner")
            .expect("couldn't find Wasm spawner service")
    };
}

/// A handle to a process.
#[repr(transparent)]
#[derive(Debug)]
pub struct Process(u32);

impl Clone for Process {
    fn clone(&self) -> Self {
        self.demote(self.get_flags())
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe { abi::process::free(self.0) }
    }
}

impl Process {
    /// Sends a message to this process.
    pub fn send(&self, data: &[u8], caps: &[&Process]) {
        let caps: Vec<u32> = caps.iter().map(|process| process.0).collect();
        unsafe {
            abi::process::send(
                self.0,
                data.as_ptr() as u32,
                data.len() as u32,
                caps.as_ptr() as u32,
                caps.len() as u32,
            );
        }
    }

    /// Sends a type, serialized as JSON, to this process.
    pub fn send_json(&self, data: &impl Serialize, caps: &[&Process]) {
        let msg = serde_json::to_string(data).unwrap();
        self.send(&msg.into_bytes(), caps);
    }

    /// Kills this process.
    pub fn kill(&self) {
        unsafe { abi::process::kill(self.0) }
    }

    /// Demotes this process handle to a handle with fewer flags.
    pub fn demote(&self, new_flags: Flags) -> Self {
        Process(unsafe { abi::process::copy(self.0, new_flags.bits()) })
    }

    /// Gets the flags for this process.
    pub fn get_flags(&self) -> Flags {
        Flags::from_bits_retain(unsafe { abi::process::get_flags(self.0) })
    }

    /// Looks up a service.
    // TODO multiple peers support; better API
    pub fn get_service(name: &str) -> Option<Self> {
        unsafe {
            let (ptr, len) = abi_string(name);
            let handle = abi::service::get(ptr, len);
            if handle == u32::MAX {
                None
            } else {
                Some(Process(handle))
            }
        }
    }

    /// Spawns a child process for the given function.
    pub fn spawn(cb: fn()) -> Self {
        WASM_SPAWNER.send_json(
            &wasm::WasmSpawnInfo {
                lump: this_lump(),
                entrypoint: Some(unsafe { std::mem::transmute::<fn(), usize>(cb) } as u32),
            },
            &[&SELF],
        );

        let signal = Signal::recv();
        let Signal::Message(mut msg) = signal else {
            panic!("expected message, received {:?}", signal);
        };

        msg.caps.remove(0)
    }
}

/// A signal.
#[derive(Clone, Debug)]
pub enum Signal {
    Unlink { subject: usize },
    Message(Message),
}

impl Signal {
    /// Blocks until a signal has been received.
    pub fn recv() -> Self {
        unsafe {
            let handle = abi::signal::recv();
            let kind = abi::signal::get_kind(handle);
            let Ok(kind) = SignalKind::try_from(kind) else {
                panic!("unknown signal kind {}", kind);
            };

            let signal = match kind {
                SignalKind::Message => Signal::Message(Message::load_from_handle(handle)),
                SignalKind::Unlink => {
                    // TODO unlink signal userdata
                    Signal::Unlink { subject: 0 }
                }
            };

            abi::signal::free(handle);
            signal
        }
    }

    /// Blocks until a signal is received or the given timeout (in microseconds)
    /// expires.
    ///
    /// Setting the timeout to 0 skips any blocking and in effect polls the signal
    /// queue for a new signal.
    pub fn recv_timeout(timeout_us: u64) -> Option<Self> {
        unsafe {
            let handle = abi::signal::recv_timeout(timeout_us);
            if handle == u32::MAX {
                None
            } else {
                let msg = Message::load_from_handle(handle);
                abi::signal::free(handle);
                Some(Signal::Message(msg))
            }
        }
    }

    /// Receives a JSON message. Panics if the next signal isn't a message or
    /// if deserialization fails.
    pub fn recv_json<T>() -> (Vec<Process>, T)
    where
        T: for<'a> Deserialize<'a>,
    {
        let signal = Signal::recv();
        let Signal::Message(msg) = signal else {
            panic!("expected message, received {:?}", signal);
        };

        let data = serde_json::from_slice(&msg.data).unwrap();
        (msg.caps, data)
    }
}

/// A message that has been received from another process.
#[derive(Clone, Debug)]
pub struct Message {
    pub data: Vec<u8>,
    pub caps: Vec<Process>,
}

impl Message {
    /// Loads a message signal by its handle.
    unsafe fn load_from_handle(handle: u32) -> Self {
        let data_len = abi::signal::get_data_len(handle) as usize;
        let mut data = Vec::with_capacity(data_len);
        data.set_len(data_len);
        abi::signal::get_data(handle, data.as_ptr() as u32);

        let caps_num = abi::signal::get_caps_num(handle) as usize;
        let mut caps = Vec::with_capacity(caps_num);
        caps.set_len(caps_num);
        abi::signal::get_caps(handle, caps.as_ptr() as u32);

        Self { data, caps }
    }
}

/// A loaded lump.
pub struct Lump(u32);

impl Drop for Lump {
    fn drop(&mut self) {
        unsafe { abi::lump::free(self.0) }
    }
}

impl Lump {
    /// Loads a new lump from in-process data.
    pub fn load(data: &[u8]) -> Self {
        unsafe {
            let ptr = data.as_ptr() as u32;
            let len = data.len() as u32;
            let handle = abi::lump::load(ptr, len);
            Self(handle)
        }
    }

    /// Loads a lump from the ID of an already existing lump.
    pub fn from_id(id: &LumpId) -> Self {
        unsafe {
            let handle = abi::lump::from_id(id as *const LumpId as u32);
            Self(handle)
        }
    }

    /// Gets the ID of this lump.
    pub fn get_id(&self) -> LumpId {
        unsafe {
            let id = LumpId(Default::default());
            let id_ptr = &id as *const LumpId as u32;
            abi::lump::get_id(self.0, id_ptr);
            id
        }
    }

    /// Retrieves the data stored in this lump.
    pub fn get_data(&self) -> Vec<u8> {
        #[allow(clippy::uninit_vec)]
        unsafe {
            let len = abi::lump::get_len(self.0) as usize;
            let mut data = Vec::with_capacity(len);
            data.set_len(len);
            abi::lump::get_data(self.0, data.as_ptr() as u32);
            data
        }
    }
}

/// Log a message.
pub fn log(level: ProcessLogLevel, module: &str, content: &str) {
    let level = level.into();
    let (module_ptr, module_len) = abi_string(module);
    let (content_ptr, content_len) = abi_string(content);
    unsafe { abi::log::log(level, module_ptr, module_len, content_ptr, content_len) }
}

#[allow(clashing_extern_declarations)]
mod abi {
    pub mod log {
        #[link(wasm_import_module = "hearth::log")]
        extern "C" {
            pub fn log(
                level: u32,
                module_ptr: u32,
                module_len: u32,
                content_ptr: u32,
                content_len: u32,
            );
        }
    }

    pub mod lump {
        #[link(wasm_import_module = "hearth::lump")]
        extern "C" {
            pub fn this_lump(ptr: u32);
            pub fn from_id(id_ptr: u32) -> u32;
            pub fn load(ptr: u32, len: u32) -> u32;
            pub fn get_id(handle: u32, id_ptr: u32);
            pub fn get_len(handle: u32) -> u32;
            pub fn get_data(handle: u32, ptr: u32);
            pub fn free(handle: u32);
        }
    }

    pub mod process {
        #[link(wasm_import_module = "hearth::process")]
        extern "C" {
            pub fn get_flags(cap: u32) -> u32;
            pub fn copy(cap: u32, new_flags: u32) -> u32;
            pub fn send(dst_cap: u32, data_ptr: u32, data_len: u32, caps_ptr: u32, caps_num: u32);
            pub fn kill(cap: u32);
            pub fn free(cap: u32);
        }
    }

    pub mod service {
        #[link(wasm_import_module = "hearth::service")]
        extern "C" {
            pub fn get(name_ptr: u32, name_len: u32) -> u32;
        }
    }

    pub mod signal {
        #[link(wasm_import_module = "hearth::signal")]
        extern "C" {
            pub fn recv() -> u32;
            pub fn recv_timeout(timeout_us: u64) -> u32;
            pub fn get_kind(msg: u32) -> u32;
            pub fn get_data_len(msg: u32) -> u32;
            pub fn get_data(msg: u32, ptr: u32);
            pub fn get_caps_num(msg: u32) -> u32;
            pub fn get_caps(msg: u32, ptr: u32);
            pub fn free(msg: u32);
        }
    }
}

#[no_mangle]
extern "C" fn _hearth_spawn_by_index(function: u32) {
    let function: fn() = unsafe { std::mem::transmute(function as usize) };
    function();
}

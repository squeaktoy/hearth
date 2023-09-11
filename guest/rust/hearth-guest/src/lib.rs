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

use std::{borrow::Borrow, marker::PhantomData};

use serde::{Deserialize, Serialize};

pub use hearth_types::*;

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct Permissions: u32 {
        const SEND = 1 << 0;
        const LINK = 1 << 1;
        const KILL = 1 << 2;
    }
}

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

/// A helper struct for request-response capabilities.
pub struct RequestResponse<Request, Response> {
    process: Process,
    _request: PhantomData<Request>,
    _response: PhantomData<Response>,
}

impl<Request, Response> RequestResponse<Request, Response>
where
    Request: Serialize,
    Response: for<'a> Deserialize<'a>,
{
    pub const fn new(process: Process) -> Self {
        Self {
            process,
            _request: PhantomData,
            _response: PhantomData,
        }
    }

    pub fn request(&self, request: Request) -> (Response, Vec<Process>) {
        let reply = Mailbox::new();
        let reply_cap = reply.make_capability(Permissions::SEND);
        reply.link(&self.process);

        self.process.send_json(&request, &[&reply_cap]);

        reply.recv_json()
    }
}

pub type Registry = RequestResponse<registry::RegistryRequest, registry::RegistryResponse>;

impl Registry {
    pub fn get_service(&self, name: &str) -> Option<Process> {
        let request = registry::RegistryRequest::Get {
            name: name.to_string(),
        };

        let (data, mut caps) = self.request(request);

        let registry::RegistryResponse::Get(present) = data else {
            panic!("failed to get service {:?}", name);
        };

        if present {
            Some(caps.remove(0))
        } else {
            None
        }
    }
}

/// A capability to the registry that this process has base access to.
pub static REGISTRY: Registry = RequestResponse::new(Process(0));

lazy_static::lazy_static! {
    /// A lazily-initialized handle to the WebAssembly spawner service.
    pub static ref WASM_SPAWNER: RequestResponse<wasm::WasmSpawnInfo, ()> = {
        RequestResponse::new(REGISTRY.get_service("hearth.cognito.WasmProcessSpawner").unwrap())
    };
}

/// A capability to a process.
#[repr(transparent)]
#[derive(Debug, Hash, PartialEq, Eq)]
pub struct Process(u32);

impl Clone for Process {
    fn clone(&self) -> Self {
        unsafe { abi::table::inc_ref(self.0) }
        Process(self.0)
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe { abi::table::dec_ref(self.0) }
    }
}

impl Process {
    /// Spawns a child process for the given function.
    pub fn spawn(cb: fn()) -> Self {
        let ((), mut caps) = WASM_SPAWNER.request(wasm::WasmSpawnInfo {
            lump: this_lump(),
            entrypoint: Some(unsafe { std::mem::transmute::<fn(), usize>(cb) } as u32),
        });

        caps.remove(0)
    }

    /// Sends a message to this process.
    pub fn send(&self, data: &[u8], caps: &[&Process]) {
        let caps: Vec<u32> = caps.iter().map(|process| (*process).borrow().0).collect();
        unsafe {
            abi::table::send(
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
        unsafe { abi::table::kill(self.0) }
    }

    /// Demotes this process handle to an owned process with fewer permissions.
    pub fn demote(&self, new_perms: Permissions) -> Process {
        let handle = unsafe { abi::table::demote(self.0, new_perms.bits()) };
        Process(handle)
    }

    /// Gets the flags for this process.
    pub fn get_flags(&self) -> Permissions {
        Permissions::from_bits_retain(unsafe { abi::table::get_permissions(self.0) })
    }
}

/// A signal.
#[derive(Clone, Debug)]
pub enum Signal {
    Unlink { subject: Process },
    Message(Message),
}

impl Signal {
    unsafe fn from_handle(handle: u32) -> Self {
        let kind = abi::mailbox::get_signal_kind(handle);

        let Ok(kind) = SignalKind::try_from(kind) else {
            panic!("unknown signal kind {}", kind);
        };

        let signal = match kind {
            SignalKind::Message => Signal::Message(Message::load_from_handle(handle)),
            SignalKind::Unlink => {
                let handle = abi::mailbox::get_unlink_capability(handle);
                let subject = Process(handle);
                Signal::Unlink { subject }
            }
        };

        abi::mailbox::destroy_signal(handle);

        signal
    }
}

/// An un-closeable mailbox that receives signals from the parent of this process.
pub static PARENT: Mailbox = Mailbox(0);

pub struct Mailbox(u32);

impl Drop for Mailbox {
    fn drop(&mut self) {
        unsafe { abi::mailbox::destroy(self.0) }
    }
}

impl Mailbox {
    pub fn new() -> Self {
        let handle = unsafe { abi::mailbox::create() };
        Self(handle)
    }

    pub fn make_capability(&self, perms: Permissions) -> Process {
        let handle = unsafe { abi::mailbox::make_capability(self.0, perms.bits()) };
        Process(handle)
    }

    pub fn link(&self, subject: &Process) {
        unsafe { abi::mailbox::link(self.0, subject.0) }
    }

    pub fn recv(&self) -> Signal {
        unsafe {
            let handle = abi::mailbox::recv(self.0);
            Signal::from_handle(handle)
        }
    }

    pub fn try_recv(&self) -> Option<Signal> {
        unsafe {
            let handle = abi::mailbox::try_recv(self.0);

            if handle == u32::MAX {
                None
            } else {
                Some(Signal::from_handle(handle))
            }
        }
    }

    /// Receives a JSON message. Panics if the next signal isn't a message or
    /// if deserialization fails.
    pub fn recv_json<T>(&self) -> (T, Vec<Process>)
    where
        T: for<'a> Deserialize<'a>,
    {
        let signal = self.recv();

        let Signal::Message(msg) = signal else {
            panic!("expected message, received {:?}", signal);
        };

        let data = serde_json::from_slice(&msg.data).unwrap();
        (data, msg.caps)
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
        let data_len = abi::mailbox::get_message_data_len(handle) as usize;
        let mut data = Vec::with_capacity(data_len);
        data.set_len(data_len);
        abi::mailbox::get_message_data(handle, data.as_ptr() as u32);

        let caps_num = abi::mailbox::get_message_caps_num(handle) as usize;
        let mut caps = Vec::with_capacity(caps_num);
        caps.set_len(caps_num);
        abi::mailbox::get_message_caps(handle, caps.as_ptr() as u32);

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

    pub mod table {
        #[link(wasm_import_module = "hearth::table")]
        extern "C" {
            pub fn inc_ref(handle: u32);
            pub fn dec_ref(handle: u32);
            pub fn get_permissions(handle: u32) -> u32;
            pub fn demote(handle: u32, perms: u32) -> u32;
            pub fn send(handle: u32, data_ptr: u32, data_len: u32, caps_ptr: u32, caps_len: u32);
            pub fn kill(handle: u32);
        }
    }

    pub mod mailbox {
        #[link(wasm_import_module = "hearth::mailbox")]
        extern "C" {
            pub fn create() -> u32;
            pub fn destroy(handle: u32);
            pub fn make_capability(handle: u32, perms: u32) -> u32;
            pub fn link(mailbox: u32, cap: u32);
            pub fn recv(handle: u32) -> u32;
            pub fn try_recv(handle: u32) -> u32;
            pub fn destroy_signal(handle: u32);
            pub fn get_signal_kind(handle: u32) -> u32;
            pub fn get_unlink_capability(handle: u32) -> u32;
            pub fn get_message_data_len(handle: u32) -> u32;
            pub fn get_message_data(handle: u32, dst_ptr: u32);
            pub fn get_message_caps_num(handle: u32) -> u32;
            pub fn get_message_caps(handle: u32, dst_ptr: u32);
        }
    }
}

#[no_mangle]
extern "C" fn _hearth_init() {
    // set panic handler that prints error to log
    std::panic::set_hook(Box::new(|info| {
        // references default_hook() from
        // https://doc.rust-lang.org/src/std/panicking.rs.html
        let location = info.location().unwrap();

        let payload = info.payload();
        let msg = if let Some(s) = payload.downcast_ref::<&'static str>() {
            *s
        } else if let Some(s) = payload.downcast_ref::<String>() {
            &s[..]
        } else {
            "Box<dyn Any>"
        };

        let log_message = format!("panicked at '{msg}', {location}");
        log(ProcessLogLevel::Error, "panic", &log_message);
    }));
}

#[no_mangle]
extern "C" fn _hearth_spawn_by_index(function: u32) {
    let function: fn() = unsafe { std::mem::transmute(function as usize) };
    function();
}

use std::marker::PhantomData;

use hearth_guest::{Capability, Mailbox, Permissions};
use serde::{Deserialize, Serialize};

pub use glam;

pub mod canvas;
pub mod debug_draw;
pub mod fs;
pub mod registry;
pub mod renderer;
pub mod terminal;
pub mod time;
pub mod wasm;
pub mod window;

/// A convenience module to import all of the most important host-side structures.
///
/// Use with:
///
/// ```rs
/// use kindling_host::prelude::*;
/// ```
pub mod prelude {
    pub use crate::{
        canvas::Canvas,
        debug_draw::DebugDraw,
        fs::{get_file, list_files, read_file},
        glam,
        registry::REGISTRY,
        terminal::Terminal,
        time::{sleep, Stopwatch, Timer},
        wasm::{spawn_fn, spawn_mod},
        window::MAIN_WINDOW,
        RequestResponse,
    };
    pub use tracing::{debug, error, info, trace, warn};
}

/// A helper struct for request-response capabilities.
pub struct RequestResponse<Request, Response> {
    cap: Capability,
    _request: PhantomData<Request>,
    _response: PhantomData<Response>,
}

impl<Request, Response> AsRef<Capability> for RequestResponse<Request, Response> {
    fn as_ref(&self) -> &Capability {
        &self.cap
    }
}

impl<Request, Response> RequestResponse<Request, Response>
where
    Request: Serialize,
    Response: for<'a> Deserialize<'a>,
{
    /// Wrap a raw capability with the request-response API.
    pub const fn new(cap: Capability) -> Self {
        Self {
            cap,
            _request: PhantomData,
            _response: PhantomData,
        }
    }

    /// Perform a request on this capability.
    ///
    /// Fails if the capability is unavailable.
    pub fn request(&self, request: Request, args: &[&Capability]) -> (Response, Vec<Capability>) {
        let reply = Mailbox::new();
        let reply_cap = reply.make_capability(Permissions::SEND);
        reply.monitor(&self.cap);

        let mut caps = Vec::with_capacity(args.len() + 1);
        caps.push(&reply_cap);
        caps.extend_from_slice(args);

        self.cap.send(&request, caps.as_slice());

        reply.recv()
    }

    /// Retrieves a [RequestResponse] service from [registry::REGISTRY] by name.
    ///
    /// Panics if the service is unavailable.
    pub fn expect_service(name: &str) -> Self {
        Self::new(
            registry::REGISTRY
                .get_service(name)
                .unwrap_or_else(|| panic!("requested service {name:?} is unavailable")),
        )
    }
}

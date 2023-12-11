use std::marker::PhantomData;

use serde::{Deserialize, Serialize};

pub use hearth_guest::{Capability, Mailbox, Permissions};

pub mod fs;
pub mod registry;
pub mod wasm;

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

        self.cap.send_json(&request, caps.as_slice());

        reply.recv_json()
    }
}

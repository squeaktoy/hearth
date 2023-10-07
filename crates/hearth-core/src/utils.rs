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

use std::{any::type_name, fmt::Debug, sync::Arc};

use async_trait::async_trait;
use flue::{CapabilityHandle, ContextSignal};
use serde::{Deserialize, Serialize};
use tracing::{debug, trace, warn};

use crate::{
    process::{Process, ProcessInfo},
    runtime::{Plugin, Runtime, RuntimeBuilder},
};

/// Helper macro to initialize [ProcessInfo] using Cargo environment variables.
///
/// This macro initializes these [ProcessInfo] fields with `CARGO_PKG_*` flags:
/// - `authors`: `CARGO_PKG_AUTHORS`
/// - `repository`: `CARGO_PKG_REPOSITORY`
/// - `homepage`: `CARGO_PKG_HOMEPAGE`
/// - `license`: `CARGO_PKG_LICENSE`
#[macro_export]
macro_rules! cargo_process_info {
    () => {{
        let mut info = ::hearth_core::process::ProcessInfo::default();

        let some_or_empty = |str: &str| {
            if str.is_empty() {
                None
            } else {
                Some(str.to_string())
            }
        };

        info.authors = some_or_empty(env!("CARGO_PKG_AUTHORS"))
            .map(|authors| authors.split(':').map(ToString::to_string).collect());

        info.repository = some_or_empty(env!("CARGO_PKG_REPOSITORY"));
        info.homepage = some_or_empty(env!("CARGO_PKG_HOMEPAGE"));
        info.license = some_or_empty(env!("CARGO_PKG_LICENSE"));
        info
    }};
}

/// Context for an incoming message in [SinkProcess].
pub struct MessageInfo<'a, T> {
    /// This process's label.
    pub label: &'a str,

    /// The [Process] that has received this message.
    pub process: &'a Process,

    /// A handle to the [Runtime] this process is running in.
    pub runtime: &'a Arc<Runtime>,

    /// The deserialized data of the message's contents.
    pub data: T,

    /// The capabilities from this message.
    pub caps: &'a [CapabilityHandle<'a>],
}

/// Context for an incoming message in [RequestResponseProcess].
pub struct RequestInfo<'a, T> {
    /// This process's label.
    pub label: &'a str,

    /// The [Process] that has received this message.
    pub process: &'a Process,

    /// The capability handle of the first capability from the message.
    pub reply: CapabilityHandle<'a>,

    /// The rest of the capabilities from the message.
    pub cap_args: &'a [CapabilityHandle<'a>],

    /// A handle to the [Runtime] this process is running in.
    pub runtime: &'a Arc<Runtime>,

    /// The deserialized data of the message's contents.
    pub data: T,
}

pub struct ResponseInfo<'a, T>
where
    T: Send,
{
    pub data: T,
    pub caps: Vec<CapabilityHandle<'a>>,
}

impl<'a, T> From<T> for ResponseInfo<'a, T>
where
    T: Send,
{
    fn from(data: T) -> Self {
        Self { data, caps: vec![] }
    }
}

impl<'a, O, E> From<E> for ResponseInfo<'a, Result<O, E>>
where
    O: Send,
    E: Send,
{
    fn from(err: E) -> Self {
        Self {
            data: Err(err),
            caps: vec![],
        }
    }
}

/// A trait for types that implement process behavior.
#[async_trait]
pub trait ProcessRunner: Send {
    /// Executes this process.
    ///
    /// Takes ownership of this object and provides a dev-facing label, a handle
    /// to the runtime, and an existing [Process] instance as context.
    async fn run(mut self, label: String, runtime: Arc<Runtime>, ctx: &Process);
}

/// A trait for process runners that continuously receive JSON-formatted messages of a single type.
///
/// This trait has a blanket implementation for [ProcessRunner] that loops and
/// receives new messages of the given data type, and calls [Self::on_message]
/// with a [RequestInfo].
#[async_trait]
pub trait SinkProcess: Send + Sync {
    /// The deserializeable data type to be received.
    type Message: for<'a> Deserialize<'a> + Send + Sync + Debug;

    /// A callback to call when messages are received by this process.
    async fn on_message<'a>(&'a mut self, message: MessageInfo<'a, Self::Message>);
}

#[async_trait]
impl<T> ProcessRunner for T
where
    T: SinkProcess,
{
    async fn run(mut self, label: String, runtime: Arc<Runtime>, ctx: &Process) {
        loop {
            let recv = ctx.borrow_parent().recv(|signal| match signal {
                ContextSignal::Message { data, caps } => Some((data.to_owned(), caps.to_owned())),
                signal => {
                    warn!("{:?} expected message but received: {:?}", label, signal);
                    None
                }
            });

            let (data, caps) = match recv.await {
                Some(Some(msg)) => msg,
                Some(None) => continue,
                None => break,
            };

            let caps: Vec<_> = caps
                .into_iter()
                .map(|cap| ctx.borrow_table().wrap_handle(cap).unwrap())
                .collect();

            let data: T::Message = match serde_json::from_slice(&data) {
                Ok(request) => request,
                Err(err) => {
                    // TODO make this a process log
                    debug!("Failed to parse {}: {:?}", type_name::<T::Message>(), err);
                    continue;
                }
            };

            trace!("{:?} received {:?}", label, data);

            self.on_message(MessageInfo {
                label: &label,
                process: &ctx,
                runtime: &runtime,
                data,
                caps: &caps,
            })
            .await;

            trace!("{:?} finished processing message", label);
        }
    }
}

#[async_trait]
pub trait RequestResponseProcess: Send + Sync {
    type Request: for<'a> Deserialize<'a> + Send + Sync + Debug;
    type Response: Serialize + Send + Debug;

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Self::Request>,
    ) -> ResponseInfo<'a, Self::Response>;
}

#[async_trait]
impl<T> SinkProcess for T
where
    T: RequestResponseProcess,
{
    type Message = T::Request;

    async fn on_message<'a>(&'a mut self, message: MessageInfo<'a, Self::Message>) {
        let Some(reply) = message.caps.first().cloned() else {
            debug!("Request to {:?} has no reply address", message.label);
            return;
        };

        let mut request = RequestInfo {
            label: message.label,
            process: message.process,
            reply: reply.clone(),
            cap_args: &message.caps[1..],
            runtime: message.runtime,
            data: message.data,
        };

        let response = self.on_request(&mut request).await;
        let data = serde_json::to_vec(&response.data).unwrap();
        let caps: Vec<_> = response.caps.iter().collect();
        let result = reply.send(&data, &caps).await;

        if let Err(err) = result {
            debug!("{:?} reply error: {:?}", message.label, err);
        }
    }
}

pub trait ServiceRunner: ProcessRunner {
    const NAME: &'static str;

    /// Gets the [ProcessInfo] for this service.
    ///
    /// The `name` field of this struct is overridden by [Self::NAME].
    fn get_process_info() -> ProcessInfo;
}

impl<T> Plugin for T
where
    T: ServiceRunner + Send + Sync + 'static,
{
    fn finalize(self, builder: &mut RuntimeBuilder) {
        let name = Self::NAME.to_string();
        let mut info = Self::get_process_info();
        info.name = Some(name.clone());
        builder.add_service(name, info, self);
    }
}

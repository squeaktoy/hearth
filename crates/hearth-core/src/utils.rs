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

use std::{any::type_name, sync::Arc};

use async_trait::async_trait;
use flue::{CapabilityHandle, ContextSignal};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::{
    process::Process,
    runtime::{Plugin, Runtime, RuntimeBuilder},
};

/// Context for an incoming message in [SinkProcess] or [RequestResponseProcess].
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
    type Message: for<'a> Deserialize<'a> + Send + Sync;

    /// A callback to call when messages are received by this process.
    async fn on_message<'a>(&'a mut self, message: &mut RequestInfo<'a, Self::Message>);
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

            let Some(reply) = caps.first().cloned() else {
                debug!("Request to {:?} has no reply address", label);
                continue;
            };

            let data: T::Message = match serde_json::from_slice(&data) {
                Ok(request) => request,
                Err(err) => {
                    // TODO make this a process log
                    debug!("Failed to parse {}: {:?}", type_name::<T::Message>(), err);
                    continue;
                }
            };

            let mut message = RequestInfo {
                label: &label,
                process: &ctx,
                reply,
                cap_args: &caps[1..],
                runtime: &runtime,
                data,
            };

            self.on_message(&mut message).await;
        }
    }
}

#[async_trait]
pub trait RequestResponseProcess: Send + Sync {
    type Request: for<'a> Deserialize<'a> + Send + Sync;
    type Response: Serialize + Send;

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

    async fn on_message<'a>(&'a mut self, message: &mut RequestInfo<'a, Self::Message>) {
        let response = self.on_request(message).await;
        let data = serde_json::to_vec(&response.data).unwrap();
        let caps: Vec<_> = response.caps.iter().collect();
        let result = message.reply.send(&data, &caps).await;

        if let Err(err) = result {
            debug!("{:?} reply error: {:?}", message.label, err);
        }
    }
}

pub trait ServiceRunner: ProcessRunner {
    const NAME: &'static str;
}

impl<T> Plugin for T
where
    T: ServiceRunner + Send + Sync + 'static,
{
    fn finish(self, builder: &mut RuntimeBuilder) {
        builder.add_service(Self::NAME.to_string(), self);
    }
}

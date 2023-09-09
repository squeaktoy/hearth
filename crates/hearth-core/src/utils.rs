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
use hearth_types::Flags;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::{
    process::{
        context::{ContextMessage, ContextSignal},
        factory::ProcessInfo,
        Process,
    },
    runtime::{Plugin, Runtime, RuntimeBuilder},
};

/// Context for an incoming message in [SinkProcess] or [RequestResponseProcess].
pub struct RequestInfo<'a, T> {
    /// The capability handle of the first capability from the message.
    pub reply: usize,

    /// The rest of the capabilities from the message.
    ///
    /// These are automatically freed after this message's callback is handled,
    /// so make a copy of it if it needs to be kept around.
    pub cap_args: &'a [usize],

    /// The [Process] that has received this message.
    pub ctx: &'a mut Process,

    /// A handle to the [Runtime] this process is running in.
    pub runtime: &'a Arc<Runtime>,

    /// The deserialized data of the message's contents.
    pub data: T,
}

pub struct ResponseInfo<T> {
    pub data: T,
    pub caps: Vec<usize>,
}

impl<T> From<T> for ResponseInfo<T> {
    fn from(data: T) -> Self {
        Self { data, caps: vec![] }
    }
}

impl<O, E> From<E> for ResponseInfo<Result<O, E>> {
    fn from(err: E) -> Self {
        Self {
            data: Err(err),
            caps: vec![],
        }
    }
}

#[async_trait]
pub trait ProcessRunner: Send + 'static {
    async fn run(mut self, label: String, runtime: Arc<Runtime>, ctx: Process);
}

/// A trait for process runners that continuously receive JSON-formatted messages of a single type.
///
/// This trait has a blanket implementation for [ProcessRunner] that loops and
/// receives new messages of the given data type, and calls [Self::on_message]
/// with a [RequestInfo].
#[async_trait]
pub trait SinkProcess: Send + Sync + 'static {
    /// The deserializeable data type to be received.
    type Message: for<'a> Deserialize<'a> + Send + Sync;

    /// A callback to call when messages are received by this process.
    async fn on_message(&mut self, message: &mut RequestInfo<'_, Self::Message>);
}

#[async_trait]
impl<T> ProcessRunner for T
where
    T: SinkProcess,
{
    async fn run(mut self, label: String, runtime: Arc<Runtime>, mut ctx: Process) {
        while let Some(signal) = ctx.recv().await {
            let ContextSignal::Message(msg) = signal else {
                // TODO make this a process log
                warn!("{:?} expected message but received: {:?}", label, signal);
                continue;
            };

            let Some(reply) = msg.caps.first().copied() else {
                // TODO make this a process log
                debug!("Request to {:?} has no reply address", label);
                continue;
            };

            let free_caps = |ctx: &mut Process| {
                for cap in msg.caps.iter() {
                    ctx.delete_capability(*cap).unwrap();
                }
            };

            let data: T::Message = match serde_json::from_slice(&msg.data) {
                Ok(request) => request,
                Err(err) => {
                    // TODO make this a process log
                    debug!("Failed to parse {}: {:?}", type_name::<T::Message>(), err);
                    free_caps(&mut ctx);
                    continue;
                }
            };

            let mut message = RequestInfo {
                reply,
                cap_args: &msg.caps[1..],
                ctx: &mut ctx,
                runtime: &runtime,
                data,
            };

            self.on_message(&mut message).await;

            free_caps(&mut ctx);
        }
    }
}

#[async_trait]
pub trait RequestResponseProcess: Send + Sync + 'static {
    type Request: for<'a> Deserialize<'a> + Send + Sync;
    type Response: Serialize;

    async fn on_request(
        &mut self,
        request: &mut RequestInfo<'_, Self::Request>,
    ) -> ResponseInfo<Self::Response>;
}

#[async_trait]
impl<T> SinkProcess for T
where
    T: RequestResponseProcess,
{
    type Message = T::Request;

    async fn on_message(&mut self, message: &mut RequestInfo<'_, Self::Message>) {
        let response = self.on_request(message).await;
        let response_data = serde_json::to_vec(&response.data).unwrap();

        message
            .ctx
            .send(
                message.reply,
                ContextMessage {
                    data: response_data,
                    caps: response.caps.clone(),
                },
            )
            .unwrap();

        for cap in response.caps {
            message.ctx.delete_capability(cap).unwrap();
        }
    }
}

pub trait ServiceRunner: ProcessRunner {
    const NAME: &'static str;
}

impl<T> Plugin for T
where
    T: ServiceRunner + Send + Sync,
{
    fn finish(self, builder: &mut RuntimeBuilder) {
        builder.add_service(
            Self::NAME.to_string(),
            ProcessInfo {},
            Flags::SEND,
            move |runtime, ctx| {
                tokio::spawn(self.run(Self::NAME.to_string(), runtime, ctx));
            },
        );
    }
}

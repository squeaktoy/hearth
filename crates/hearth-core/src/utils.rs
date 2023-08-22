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

pub struct RequestInfo<'a, T> {
    pub reply: usize,
    pub cap_args: &'a [usize],
    pub ctx: &'a mut Process,
    pub runtime: &'a Arc<Runtime>,
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
pub trait RequestResponseProcess: Send + 'static {
    type Request: for<'a> Deserialize<'a> + Send;
    type Response: Serialize;

    async fn on_request(
        &mut self,
        request: RequestInfo<'_, Self::Request>,
    ) -> ResponseInfo<Self::Response>;
}

pub trait RequestResponseService: RequestResponseProcess {
    const NAME: &'static str;
}

impl<T> Plugin for T
where
    T: RequestResponseService + Send + Sync,
{
    fn finish(self, builder: &mut RuntimeBuilder) {
        builder.add_service(
            Self::NAME.to_string(),
            ProcessInfo {},
            Flags::SEND,
            move |runtime, ctx| {
                tokio::spawn(run_request_response_process(
                    runtime,
                    ctx,
                    Self::NAME.to_string(),
                    self,
                ));
            },
        );
    }
}

pub async fn run_request_response_process<T>(
    runtime: Arc<Runtime>,
    mut ctx: Process,
    label: String,
    mut process: T,
) where
    T: RequestResponseProcess,
{
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

        let data: T::Request = match serde_json::from_slice(&msg.data) {
            Ok(request) => request,
            Err(err) => {
                // TODO make this a process log
                debug!("Failed to parse {}: {:?}", type_name::<T::Request>(), err);

                free_caps(&mut ctx);
                continue;
            }
        };

        let request = RequestInfo {
            reply,
            cap_args: &msg.caps[1..],
            ctx: &mut ctx,
            runtime: &runtime,
            data,
        };

        let response = process.on_request(request).await;
        let response_data = serde_json::to_vec(&response.data).unwrap();

        ctx.send(
            reply,
            ContextMessage {
                data: response_data,
                caps: response.caps.clone(),
            },
        )
        .unwrap();

        free_caps(&mut ctx);

        for cap in response.caps {
            ctx.delete_capability(cap).unwrap();
        }
    }
}

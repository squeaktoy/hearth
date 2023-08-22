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

#[async_trait]
pub trait RequestResponseService: Send + 'static {
    const NAME: &'static str;
    type Request: for<'a> Deserialize<'a> + Send;
    type Response: Serialize;

    async fn on_request(
        &mut self,
        request: RequestInfo<'_, Self::Request>,
    ) -> anyhow::Result<Self::Response>;
}

impl<T> Plugin for T
where
    T: RequestResponseService + Send + Sync,
{
    fn finish(self, builder: &mut RuntimeBuilder) {
        add_request_response_service(builder, Self::NAME, self);
    }
}

pub fn add_request_response_service<T>(
    builder: &mut RuntimeBuilder,
    name: impl ToString,
    mut service: T,
) where
    T: RequestResponseService,
{
    let name = name.to_string();
    builder.add_service(
        name.clone(),
        ProcessInfo {},
        Flags::SEND,
        move |runtime, mut ctx| {
            tokio::spawn(async move {
                while let Some(signal) = ctx.recv().await {
                    let ContextSignal::Message(msg) = signal else {
                        // TODO make this a process log
                        warn!("{:?} expected message but received: {:?}", name, signal);
                        continue;
                    };

                    let Some(reply) = msg.caps.first().copied() else {
                        // TODO make this a process log
                        debug!("Request to {:?} has no reply address", name);
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

                    let response = match service.on_request(request).await {
                        Ok(response) => response,
                        Err(err) => {
                            // TODO make this a process log
                            debug!("Request to {:?} failed: {:?}", name, err);
                            free_caps(&mut ctx);
                            continue;
                        }
                    };

                    let response_data = serde_json::to_vec(&response).unwrap();

                    ctx.send(
                        reply,
                        ContextMessage {
                            data: response_data,
                            caps: vec![],
                        },
                    )
                    .unwrap();

                    free_caps(&mut ctx);
                }
            });
        },
    );
}

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

use std::sync::Arc;

use hearth_core::{
    async_trait,
    hearth_types::Flags,
    process::{context::ContextSignal, factory::ProcessInfo, Process},
    runtime::{Plugin, Runtime, RuntimeBuilder},
    tokio::{spawn, sync::oneshot::Sender},
};

struct Hook {
    service: String,
    callback: Sender<(Process, usize)>,
}

pub struct InitPlugin {
    hooks: Vec<Hook>,
}

#[async_trait]
impl Plugin for InitPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        for hook in std::mem::take(&mut self.hooks) {
            builder.add_service(
                hook.service,
                ProcessInfo {},
                Flags::SEND,
                move |_runtime, mut ctx| {
                    spawn(async move {
                        while let Some(signal) = ctx.recv().await {
                            let ContextSignal::Message(msg) = signal else {
                                tracing::error!("expected message, got {:?}", signal);
                                continue;
                            };

                            let Some(init_cap) = msg.caps.first() else {
                                continue;
                            };

                            let _ = hook.callback.send((ctx, *init_cap));
                            break;
                        }
                    });
                },
            );
        }
    }

    async fn run(&mut self, runtime: Arc<Runtime>) {}
}

impl InitPlugin {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn add_hook(&mut self, service: String, callback: Sender<(Process, usize)>) {
        self.hooks.push(Hook { service, callback });
    }
}

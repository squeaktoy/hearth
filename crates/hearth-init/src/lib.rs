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

use std::path::PathBuf;

use hearth_core::{
    hearth_types::{wasm::WasmSpawnInfo, Flags},
    process::{
        context::{ContextMessage, ContextSignal},
        factory::ProcessInfo,
        Process,
    },
    runtime::{Plugin, RuntimeBuilder},
    tokio::{spawn, sync::oneshot::Sender},
};
use tracing::debug;

struct Hook {
    service: String,
    callback: Sender<(Process, usize)>,
}

pub struct InitPlugin {
    init_path: PathBuf,
    hooks: Vec<Hook>,
}

impl Plugin for InitPlugin {
    fn finish(mut self, builder: &mut RuntimeBuilder) {
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

        builder.add_runner(move |runtime| {
            spawn(async move {
                debug!("Loading init system module");
                let wasm_data = std::fs::read(self.init_path.clone()).unwrap();
                let wasm_lump = runtime.lump_store.add_lump(wasm_data.into()).await;

                debug!("Running init system");
                let mut parent = runtime.process_factory.spawn(ProcessInfo {}, Flags::SEND);
                let wasm_spawner = parent
                    .get_service("hearth.cognito.WasmProcessSpawner")
                    .expect("Wasm spawner service not found");

                let spawn_info = WasmSpawnInfo {
                    lump: wasm_lump,
                    entrypoint: None,
                };

                parent
                    .send(
                        wasm_spawner,
                        ContextMessage {
                            data: serde_json::to_vec(&spawn_info).unwrap(),
                            caps: vec![0],
                        },
                    )
                    .unwrap();
            });
        });
    }
}

impl InitPlugin {
    pub fn new(init_path: PathBuf) -> Self {
        Self {
            init_path,
            hooks: Vec::new(),
        }
    }

    pub fn add_hook(&mut self, service: String, callback: Sender<(Process, usize)>) {
        self.hooks.push(Hook { service, callback });
    }
}

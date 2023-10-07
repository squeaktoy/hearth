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

use std::{path::PathBuf, sync::Arc};

use hearth_core::{
    async_trait, cargo_process_info,
    flue::{ContextSignal, OwnedCapability, Permissions},
    hearth_types::{registry::RegistryRequest, wasm::WasmSpawnInfo},
    process::Process,
    runtime::{Plugin, Runtime, RuntimeBuilder},
    tokio::{spawn, sync::oneshot::Sender},
    utils::ProcessRunner,
};
use tracing::debug;

struct Hook {
    service: String,
    callback: Sender<OwnedCapability>,
}

#[async_trait]
impl ProcessRunner for Hook {
    async fn run(mut self, _label: String, _runtime: Arc<Runtime>, ctx: &Process) {
        while let Some(hook) = ctx
            .borrow_parent()
            .recv(|signal| {
                let ContextSignal::Message { data: _, caps } = signal else {
                    tracing::error!("expected message, got {:?}", signal);
                    return None;
                };

                let Some(init_cap) = caps.first() else {
                    return None;
                };

                Some(*init_cap)
            })
            .await
        {
            if let Some(init_cap) = hook {
                let cap = ctx.borrow_table().get_owned(init_cap).unwrap();
                let _ = self.callback.send(cap);
                return;
            }
        }
    }
}

pub struct InitPlugin {
    init_path: PathBuf,
    hooks: Vec<Hook>,
}

impl Plugin for InitPlugin {
    fn finalize(self, builder: &mut RuntimeBuilder) {
        for hook in self.hooks {
            let mut info = cargo_process_info!();
            info.name = Some(hook.service.clone());
            info.description = Some("An init hook. Send a message with no data and a single capability to initialize it.".to_string());

            builder.add_service(hook.service.clone(), info, hook);
        }

        builder.add_runner(move |runtime| {
            spawn(async move {
                debug!("Loading init system module");
                let wasm_data = std::fs::read(self.init_path.clone()).unwrap();
                let wasm_lump = runtime.lump_store.add_lump(wasm_data.into()).await;

                let spawn_info = WasmSpawnInfo {
                    lump: wasm_lump,
                    entrypoint: None,
                };

                debug!("Running init system");
                let mut info = cargo_process_info!();
                info.name = Some("init system parent".to_string());

                let parent = runtime.process_factory.spawn(info);
                let response = parent.borrow_store().create_mailbox().unwrap();
                let response_cap = response.make_capability(Permissions::SEND);

                let registry = runtime.registry.borrow_parent();
                let table = parent.borrow_table();
                let perms = Permissions::SEND | Permissions::LINK;
                let registry = table.import(registry, perms);
                let registry = table.wrap_handle(registry).unwrap();

                let request = RegistryRequest::Get {
                    name: "hearth.cognito.WasmProcessSpawner".to_string(),
                };

                registry
                    .send(&serde_json::to_vec(&request).unwrap(), &[&response_cap])
                    .await
                    .unwrap();

                let spawner = response
                    .recv(|signal| {
                        let ContextSignal::Message { mut caps, .. } = signal else {
                            panic!("expected message, got {:?}", signal);
                        };

                        caps.remove(0)
                    })
                    .await
                    .unwrap();

                let spawner = parent.borrow_table().wrap_handle(spawner).unwrap();

                spawner
                    .send(
                        &serde_json::to_vec(&spawn_info).unwrap(),
                        &[&response_cap, &registry],
                    )
                    .await
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

    pub fn add_hook(&mut self, service: String, callback: Sender<OwnedCapability>) {
        self.hooks.push(Hook { service, callback });
    }
}

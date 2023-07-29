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

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use hearth_core::{
    async_trait,
    hearth_types::{fs::*, Flags, ProcessLogLevel},
    lump::LumpStoreImpl,
    process::{
        context::{ContextMessage, ContextSignal},
        factory::{ProcessInfo, ProcessLogEvent},
        Process,
    },
    runtime::{Plugin, Runtime, RuntimeBuilder},
    tracing::warn,
};

async fn serve(root: PathBuf, mut ctx: Process, lumps: Arc<LumpStoreImpl>) {
    while let Some(signal) = ctx.recv().await {
        let ContextSignal::Message(message) = signal else {
            panic!("expected message signal; got {:?}", signal);
        };

        // TODO share this code with hearth-cognito in hearth-core
        let request: Request = match serde_json::from_slice(&message.data) {
            Ok(message) => message,
            Err(err) => {
                ctx.log(ProcessLogEvent {
                    level: ProcessLogLevel::Error,
                    module: "WasmProcessSpawner".to_string(),
                    content: format!("Failed to parse WasmSpawnInfo: {:?}", err),
                });

                warn!("Failed to parse WasmSpawnInfo: {:?}", err);

                continue;
            }
        };

        let response = on_request(&root, request, lumps.as_ref()).await;
        let response = serde_json::to_vec(&response).unwrap();
        let Some(reply) = message.caps.first().copied() else {
            continue;
        };

        ctx.send(
            reply,
            ContextMessage {
                data: response,
                caps: vec![],
            },
        )
        .unwrap();

        for unused in message.caps {
            ctx.delete_capability(unused).unwrap();
        }
    }
}

async fn on_request(root: &Path, request: Request, lumps: &LumpStoreImpl) -> Response {
    let target = PathBuf::try_from(request.target).map_err(|_| Error::InvalidTarget)?;
    let mut path = root.to_path_buf();
    for component in target.components() {
        match component {
            std::path::Component::Normal(normal) => path.push(normal),
            _ => return Err(Error::DirectoryTraversal),
        }
    }

    match request.kind {
        RequestKind::Get => {
            let contents = match std::fs::read(path) {
                Ok(contents) => contents,
                Err(_) => todo!(),
            };

            let lump = lumps.add_lump(contents.into()).await;
            Ok(Success::Get(lump))
        }
        RequestKind::List => {
            let dirs = match std::fs::read_dir(path) {
                Ok(dirs) => dirs,
                Err(_) => todo!(),
            };

            let dirs: Vec<_> = dirs
                .into_iter()
                .map(|dir| {
                    let dir = dir.unwrap();

                    FileInfo {
                        name: dir.file_name().to_string_lossy().to_string(),
                    }
                })
                .collect();

            Ok(Success::List(dirs))
        }
    }
}

pub struct FsPlugin {
    root: PathBuf,
}

#[async_trait]
impl Plugin for FsPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        let root = self.root.clone();

        builder.add_service(
            "hearth.fs.Filesystem".into(),
            ProcessInfo {},
            Flags::SEND,
            move |runtime, process| {
                hearth_core::tokio::spawn(async move {
                    serve(root, process, runtime.lump_store.clone()).await;
                });
            },
        );
    }

    async fn run(&mut self, _runtime: Arc<Runtime>) {}
}

impl FsPlugin {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

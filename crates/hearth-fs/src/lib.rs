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

use hearth_core::{
    async_trait, cargo_process_metadata, hearth_types::fs::*, process::ProcessMetadata, utils::*,
};
use std::fs::{read, read_dir};
use std::path::{Component, PathBuf};

pub struct FsPlugin {
    root: PathBuf,
}

#[async_trait]
impl RequestResponseProcess for FsPlugin {
    type Request = Request;
    type Response = Response;

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Request>,
    ) -> ResponseInfo<'a, Response> {
        match self.handle_request(request).await {
            Err(e) => e.into(),
            s => ResponseInfo {
                data: s,
                caps: vec![],
            },
        }
    }
}

impl ServiceRunner for FsPlugin {
    const NAME: &'static str = "hearth.fs.Filesystem";

    fn get_process_metadata() -> ProcessMetadata {
        let mut meta = cargo_process_metadata!();
        meta.description =
            Some("The native filesystem access service. Accepts FsRequest.".to_string());
        meta
    }
}

impl FsPlugin {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    async fn handle_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, Request>,
    ) -> Result<Success, Error> {
        let target = PathBuf::try_from(&request.data.target).map_err(|_| Error::InvalidTarget)?;

        let mut path = self.root.to_path_buf();
        for component in target.components() {
            match component {
                Component::Normal(normal) => path.push(normal),
                _ => return Err(Error::DirectoryTraversal),
            }
        }

        match request.data.kind {
            RequestKind::Get => {
                let contents = read(path).map_err(|_| Error::NotFound)?;

                let lump = request.runtime.lump_store.add_lump(contents.into()).await;

                Ok(Success::Get(lump))
            }
            RequestKind::List => {
                // read_dir can have multiple error differnt types of errors and
                // im not sure of a good way to handle individual each one.
                let dirs = read_dir(path).map_err(|_| Error::DirectoryError)?;

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
}

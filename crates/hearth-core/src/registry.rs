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

use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use flue::{Mailbox, Permissions, PostOffice, Table};
use hearth_types::registry::*;
use tracing::warn;

use crate::{
    process::Process,
    runtime::Runtime,
    utils::{ProcessRunner, RequestInfo, RequestResponseProcess, ResponseInfo},
};

pub struct RegistryBuilder {
    table: Table,
    inner: Registry,
}

impl RegistryBuilder {
    pub fn new(post: Arc<PostOffice>) -> Self {
        Self {
            table: Table::new(post),
            inner: Registry::default(),
        }
    }

    pub fn add(&mut self, name: String, mailbox: &Mailbox) {
        let perms = Permissions::SEND | Permissions::LINK;
        let cap = self.table.import(mailbox, perms);

        if self.inner.services.contains_key(&name) {
            warn!("attempted to add service {:?} again", name);
        } else {
            self.inner.services.insert(name, cap);
        }
    }

    pub async fn run(self, runtime: Arc<Runtime>) {
        let ctx = Process::from(self.table);
        self.inner.run("Registry".to_string(), runtime, ctx).await;
    }
}

#[derive(Default)]
struct Registry {
    services: HashMap<String, usize>,
}

#[async_trait]
impl RequestResponseProcess for Registry {
    type Request = RegistryRequest;
    type Response = RegistryResponse;

    async fn on_request<'a>(
        &'a mut self,
        request: &mut RequestInfo<'a, RegistryRequest>,
    ) -> ResponseInfo<'a, Self::Response> {
        use RegistryRequest::*;
        match &request.data {
            Get { name } => {
                if let Some(handle) = self.services.get(name) {
                    let cap = request.process.with_table(|table| {
                        table.inc_ref(*handle).unwrap();
                        table.wrap_handle(*handle).unwrap()
                    });

                    ResponseInfo {
                        data: RegistryResponse::Get(true),
                        caps: vec![cap],
                    }
                } else {
                    ResponseInfo {
                        data: RegistryResponse::Get(false),
                        caps: vec![],
                    }
                }
            }
            Register { .. } => ResponseInfo {
                data: RegistryResponse::Register(None),
                caps: vec![],
            },
            List => ResponseInfo {
                data: RegistryResponse::List(
                    self.services.keys().map(ToString::to_string).collect(),
                ),
                caps: vec![],
            },
        }
    }
}

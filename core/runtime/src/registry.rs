use std::{
    collections::{hash_map, HashMap},
    sync::Arc,
};

use async_trait::async_trait;
use flue::{CapabilityHandle, Mailbox, Permissions, PostOffice, Table};
use hearth_schema::registry::*;
use tracing::warn;

use crate::utils::{RequestInfo, RequestResponseProcess, ResponseInfo};

/// A builder to initialize the service entries in a [Registry], since they
/// can't be modified once the registry has started.
pub struct RegistryBuilder {
    pub table: Table,
    pub inner: Registry,
}

impl RegistryBuilder {
    /// Creates a new registry builder for the given post office.
    pub fn new(post: Arc<PostOffice>) -> Self {
        Self {
            table: Table::new(post),
            inner: Registry::default(),
        }
    }

    /// Adds a service by its serving mailbox to this registry.
    ///
    /// The capability has the send permission so that it can receive requests,
    /// and the link permission so that users of the services can observe if
    /// the service becomes unavailable.
    ///
    /// Logs a warning if the name is already taken.
    pub fn add(&mut self, name: String, mailbox: &Mailbox) {
        let perms = Permissions::SEND | Permissions::MONITOR;
        // Panic if table has a different post office than mailbox
        let cap = mailbox.export_to(perms, &self.table).unwrap();

        if let hash_map::Entry::Vacant(entry) = self.inner.services.entry(name.clone()) {
            entry.insert(cap.into_handle());
        } else {
            warn!("attempted to add service {:?} again", name);
        }
    }
}

/// A host-side implementation of an immutable registry.
///
/// A Hearth registry is a process that stores capabilities to other processes
/// by names, which are user-friendly strings. Then, it provides those
/// capabilities to other processes who request access to those capabilities
/// using their names. The capabilities stored in a registry are referred to
/// as "services".
///
/// This registry implementation is constructed using [RegistryBuilder] and is
/// immutable once created.
#[derive(Default)]
pub struct Registry {
    services: HashMap<String, CapabilityHandle>,
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

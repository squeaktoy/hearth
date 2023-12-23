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

use std::collections::HashMap;

use hearth_guest::{
    registry::{RegistryRequest, RegistryResponse},
    Capability, PARENT,
};
use kindling_host::prelude::*;
use serde::{Deserialize, Serialize};

// TODO: Break out to schema crate
#[derive(Deserialize, Serialize)]
pub struct RegistryConfig {
    pub service_names: Vec<String>,
}

pub struct Registry {
    cap: Capability,
}

impl Registry {
    /// Spawn a new immutable registry.
    pub fn spawn(services: Vec<(String, Capability)>) -> Self {
        let (service_names, caps): (Vec<String>, Vec<Capability>) = services.into_iter().unzip();
        let caps: Vec<&Capability> = caps.iter().collect();
        let config = RegistryConfig { service_names };
        let registry = spawn_fn(Self::on_spawn, None);
        registry.send_json(&config, &caps);
        Registry { cap: registry }
    }

    fn on_spawn() {
        let (config, service_list) = PARENT.recv_json::<RegistryConfig>();

        // Hashmap that maps the service names to their capabilities
        let mut services = HashMap::new();
        for (cap, name) in service_list.iter().zip(config.service_names) {
            info!("now serving {:?}", name);
            services.insert(name, cap);
        }

        loop {
            let (request, caps) = PARENT.recv_json::<RegistryRequest>();
            let Some(reply) = caps.first() else {
                debug!("Request did not contain a capability");
                continue;
            };

            let mut response_cap = vec![];
            use RegistryRequest::*;
            let response = match request {
                Get { name } => match services.get(&name) {
                    Some(service) => {
                        response_cap.push(*service);
                        RegistryResponse::Get(true)
                    }
                    None => {
                        info!("Requested service \"{name}\" not found");
                        RegistryResponse::Get(false)
                    }
                },
                Register { .. } => {
                    debug!("Attempted to register on an immutable registry");
                    RegistryResponse::Register(None)
                }
                List => RegistryResponse::List(services.keys().map(|k| k.to_string()).collect()),
            };

            reply.send_json(&response, &response_cap)
        }
    }
}

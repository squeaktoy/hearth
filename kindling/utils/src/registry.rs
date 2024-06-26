use std::collections::HashMap;

use hearth_guest::{
    registry::{RegistryRequest, RegistryResponse},
    Capability, PARENT,
};
use kindling_host::{prelude::*, registry::Registry};
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize)]
pub struct RegistryConfig {
    pub service_names: Vec<String>,
}

pub struct RegistryServer {
    services: HashMap<String, Capability>,
}

impl RegistryServer {
    /// Spawn a new immutable registry.
    pub fn spawn(services: Vec<(String, Capability)>) -> Registry {
        let (service_names, caps): (Vec<String>, Vec<Capability>) = services.into_iter().unzip();
        let caps: Vec<&Capability> = caps.iter().collect();
        let config = RegistryConfig { service_names };
        let registry = spawn_fn(Self::init, None);
        registry.send(&config, &caps);
        RequestResponse::new(registry)
    }

    fn init() {
        let (config, service_list) = PARENT.recv::<RegistryConfig>();

        // Hashmap that maps the service names to their capabilities
        let mut services = HashMap::new();
        for (cap, name) in service_list.iter().zip(config.service_names) {
            info!("now serving {:?}", name);
            services.insert(name, cap.clone());
        }
        let registry = RegistryServer { services };

        loop {
            let (request, caps) = PARENT.recv::<RegistryRequest>();
            let Some(reply) = caps.first() else {
                debug!("Request did not contain a capability");
                continue;
            };
            let (response, response_cap) = registry.on_request(request);
            reply.send(&response, &response_cap)
        }
    }

    fn on_request(&self, request: RegistryRequest) -> (RegistryResponse, Vec<&Capability>) {
        use RegistryRequest::*;
        match request {
            Get { name } => match self.services.get(&name) {
                Some(service) => (RegistryResponse::Get(true), vec![service]),
                None => {
                    info!("Requested service \"{name}\" not found");
                    (RegistryResponse::Get(false), vec![])
                }
            },
            Register { .. } => {
                debug!("Attempted to register on an immutable registry");
                (RegistryResponse::Register(None), vec![])
            }
            List => (
                RegistryResponse::List(self.services.keys().map(|k| k.to_string()).collect()),
                vec![],
            ),
        }
    }
}

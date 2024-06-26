use super::*;

use hearth_guest::{
    registry::{self, RegistryRequest, RegistryResponse},
    Capability,
};

/// A wrapper for capabilities implementing the [registry] protocol.
pub type Registry = RequestResponse<registry::RegistryRequest, registry::RegistryResponse>;

impl Registry {
    /// Gets a service by its name. Returns `None` if the service doesn't exist.
    pub fn get_service(&self, name: &str) -> Option<Capability> {
        let request = registry::RegistryRequest::Get {
            name: name.to_string(),
        };

        let (data, mut caps) = self.request(request, &[]);

        let registry::RegistryResponse::Get(present) = data else {
            panic!("failed to get service {:?}", name);
        };

        if present {
            Some(caps.remove(0))
        } else {
            None
        }
    }

    /// Lists all services in this registry.
    pub fn list_services(&self) -> Vec<String> {
        let (data, _) = self.request(RegistryRequest::List, &[]);
        let RegistryResponse::List(list) = data else {
            panic!("failed to list services");
        };
        list
    }
}

/// A capability to the registry that this process has base access to.
pub static REGISTRY: Registry = RequestResponse::new(unsafe { Capability::new_raw(0) });

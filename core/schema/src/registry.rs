use serde::{Deserialize, Serialize};

/// A message schema for messages sent to a registry process. All variants require
/// that a reply cap is the first capability in the message.
///
/// Compliant registry processes will reply with a [RegistryResponse].
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum RegistryRequest {
    /// Gets a service by name. Returns [RegistryResponse::Get].
    Get { name: String },

    /// Registers the second capability in the message with the given name.
    /// Returns [RegistryResponse::Register].
    Register { name: String },

    /// Requests a list of all of the registered services. Returns
    /// [RegistryReponse::List].
    List,
}

/// A response to a [RegistryRequest].
#[derive(Clone, Debug, Deserialize, Serialize)]
pub enum RegistryResponse {
    /// If true, returns the service with the requested name with the first
    /// capability, if false, the service is unavailable and no cap is given.
    Get(bool),

    /// Returns one of the following:
    /// - `Some(true)`: the service has been successfully registered and there
    ///   was an old service present.
    /// - `Some(false)`: the service has been successfully registered and no
    ///   service has been replaced.
    /// - `None`: this registry is read-only and the service has not been
    ///   registered.
    Register(Option<bool>),

    /// Returns a list of the names of all services in this registry.
    List(Vec<String>),
}

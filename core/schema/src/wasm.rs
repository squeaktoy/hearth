use crate::LumpId;
use serde::{Deserialize, Serialize};

/// A spawn message sent to the Wasm process spawner service.
///
/// The service replies with a message containing the decimal representation of
/// the new process's local process ID.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WasmSpawnInfo {
    /// The [LumpId] of the Wasm module lump source.
    pub lump: LumpId,

    /// The identifier of the entrypoint to execute. If not specified, runs
    /// the exported "run" function.
    pub entrypoint: Option<u32>,
}

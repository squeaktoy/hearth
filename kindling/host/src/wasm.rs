use super::*;

use hearth_guest::{wasm::*, LumpId};

/// Spawns a child process for the given function.
pub fn spawn_fn(cb: fn(), registry: Option<Capability>) -> Result<Capability, ()> {
    // directly transmute a Rust function pointer to a Wasm function index
    let entrypoint = unsafe { std::mem::transmute::<fn(), usize>(cb) } as u32;

    let ((), caps) = WASM_SPAWNER.request(
        wasm::WasmSpawnInfo {
            lump: hearth_guest::this_lump(),
            entrypoint: Some(entrypoint),
        },
        &[registry.as_ref().unwrap_or(registry::REGISTRY.as_ref())],
    );

    caps.get(0).cloned().ok_or(())
}

pub fn spawn_mod(lump: LumpId, registry: Option<Capability>) -> Result<Capability, ()> {
    let ((), caps) = WASM_SPAWNER.request(
        wasm::WasmSpawnInfo {
            lump,
            entrypoint: None,
        },
        &[registry.as_ref().unwrap_or(registry::REGISTRY.as_ref())],
    );
    caps.get(0).cloned().ok_or(())
}

lazy_static::lazy_static! {
    /// A lazily-initialized handle to the WebAssembly spawner service.
    pub static ref WASM_SPAWNER: RequestResponse<wasm::WasmSpawnInfo, ()> = {
        RequestResponse::new(registry::REGISTRY.get_service("hearth.wasm.WasmProcessSpawner").unwrap())
    };
}

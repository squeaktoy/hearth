use std::sync::Arc;

use hearth_core::process::{Process, ProcessContext};
use hearth_core::runtime::{Plugin, Runtime, RuntimeBuilder};
use hearth_macros::impl_wasm_linker;
use hearth_rpc::{remoc, ProcessInfo};
use hearth_wasm::{GuestMemory, WasmLinker};
use remoc::rtc::async_trait;
use tracing::{debug, error};
use wasmtime::*;

/// This contains all script-accessible process-related stuff.
pub struct Cognito {
    ctx: ProcessContext,
}

// Should automatically generate link_print_hello_world:
// #[impl_wasm_linker]
// should work for any struct, not just Cognito
#[impl_wasm_linker(module = "cognito")]
impl Cognito {
    pub fn this_pid(&self) -> u64 {
        self.ctx.get_pid().0
    }

    pub fn service_lookup(
        &self,
        mut memory: GuestMemory<'_>,
        peer: u32,
        name_ptr: u32,
        name_len: u32,
    ) -> u64 {
        unimplemented!()
    }

    pub fn service_register(
        &self,
        mut memory: GuestMemory<'_>,
        pid: u64,
        name_ptr: u32,
        name_len: u32,
    ) {
        unimplemented!()
    }

    pub fn service_deregister(
        &self,
        mut memory: GuestMemory<'_>,
        peer: u32,
        name_ptr: u32,
        name_len: u32,
    ) {
        unimplemented!()
    }

    pub async fn kill(&self, pid: u64) {
        unimplemented!()
    }

    pub async fn send(&self, mut memory: GuestMemory<'_>, pid: u64, ptr: u32, len: u32) {
        unimplemented!()
    }

    pub async fn recv(&self) {
        unimplemented!()
    }

    pub async fn recv_timeout(&self, timeout_us: u64) {
        unimplemented!()
    }

    pub fn message_get_sender(&self, msg: u32) -> u64 {
        unimplemented!()
    }

    pub fn message_get_len(&self, msg: u32) -> u32 {
        unimplemented!()
    }

    pub fn message_get_data(&self, mut memory: GuestMemory<'_>, msg: u32, ptr: u32) {
        unimplemented!()
    }
}

struct ProcessData {
    cognito: Cognito,
}

impl AsRef<Cognito> for ProcessData {
    fn as_ref(&self) -> &Cognito {
        &self.cognito
    }
}

struct WasmProcess {
    engine: Arc<Engine>,
    linker: Arc<Linker<ProcessData>>,
    module: Arc<Module>,
}

#[async_trait]
impl Process for WasmProcess {
    fn get_info(&self) -> ProcessInfo {
        ProcessInfo {}
    }

    async fn run(&mut self, ctx: ProcessContext) {
        // TODO log using the process log instead of tracing?
        let cognito = Cognito { ctx };
        let data = ProcessData { cognito };
        let mut store = Store::new(&self.engine, data);
        let instance = match self
            .linker
            .instantiate_async(&mut store, &self.module)
            .await
        {
            Ok(instance) => instance,
            Err(err) => {
                error!("Failed to instantiate WasmProcess: {:?}", err);
                return;
            }
        };

        // TODO better wasm invocation?
        match instance.get_typed_func::<(), ()>(&mut store, "run") {
            Ok(run) => {
                if let Err(err) = run.call_async(&mut store, ()).await {
                    error!("Wasm run error: {:?}", err);
                }
            }
            Err(err) => {
                error!("Couldn't find run function: {:?}", err);
            }
        }
    }
}

pub struct WasmProcessSpawner {
    engine: Arc<Engine>,
    linker: Arc<Linker<ProcessData>>,
}

#[async_trait]
impl Process for WasmProcessSpawner {
    fn get_info(&self) -> ProcessInfo {
        ProcessInfo {}
    }

    async fn run(&mut self, mut ctx: ProcessContext) {
        while let Some(message) = ctx.recv().await {
            debug!("WasmProcessSpawner: got message from {:?}", message.sender);
        }
    }
}

impl WasmProcessSpawner {
    pub fn new() -> Self {
        let mut config = Config::new();
        config.async_support(true);

        let engine = Engine::new(&config).unwrap();
        let mut linker = Linker::new(&engine);
        Cognito::add_to_linker(&mut linker);

        Self {
            engine: Arc::new(engine),
            linker: Arc::new(linker),
        }
    }
}

pub struct WasmPlugin {}

#[async_trait]
impl Plugin for WasmPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        let name = "hearth.cognito.WasmProcessSpawner".to_string();
        let spawner = WasmProcessSpawner::new();
        builder.add_service(name, spawner);
    }

    async fn run(&mut self, _runtime: Arc<Runtime>) {
        // WasmProcessSpawner takes care of everything
    }
}

impl WasmPlugin {
    pub fn new() -> Self {
        Self {}
    }
}

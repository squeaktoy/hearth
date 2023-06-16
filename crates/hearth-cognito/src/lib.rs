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

use std::sync::Arc;

use anyhow::{anyhow, Result};
use hearth_core::anyhow::{self, bail, Context};
use hearth_core::asset::{AssetLoader, AssetStore};
use hearth_core::lump::{bytes::Bytes, LumpStoreImpl};
use hearth_core::process::context::{ContextMessage, ContextSignal, Flags};
use hearth_core::process::Process;
use hearth_core::runtime::{Plugin, Runtime, RuntimeBuilder};
use hearth_core::tokio;
use hearth_macros::impl_wasm_linker;
use hearth_rpc::hearth_types::wasm::WasmSpawnInfo;
use hearth_rpc::hearth_types::{LumpId, ProcessLogLevel};
use hearth_rpc::{remoc, ProcessInfo, ProcessLogEvent};
use hearth_wasm::{GuestMemory, WasmLinker};
use remoc::rtc::async_trait;
use slab::Slab;
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, error, warn};
use wasmtime::*;

/// Implements the `hearth::log` ABI module.
pub struct LogAbi {
    pub ctx: Arc<Mutex<Process>>,
}

#[impl_wasm_linker(module = "hearth::log")]
impl LogAbi {
    async fn log(
        &self,
        memory: GuestMemory<'_>,
        level: u32,
        module_ptr: u32,
        module_len: u32,
        content_ptr: u32,
        content_len: u32,
    ) -> Result<()> {
        self.ctx.lock().await.log(ProcessLogEvent {
            level: level
                .try_into()
                .map_err(|_| anyhow!("invalid log level constant {}", level))?,
            module: memory.get_str(module_ptr, module_len)?.to_string(),
            content: memory.get_str(content_ptr, content_len)?.to_string(),
        });

        Ok(())
    }
}

impl LogAbi {
    pub fn new(ctx: Arc<Mutex<Process>>) -> Self {
        Self { ctx }
    }
}

/// A script-local lump stored in [LumpAbi].
#[derive(Debug)]
pub struct LocalLump {
    pub id: LumpId,
    pub bytes: Bytes,
}

/// Implements the `hearth::lump` ABI module.
#[derive(Debug)]
pub struct LumpAbi {
    pub lump_store: Arc<LumpStoreImpl>,
    pub lump_handles: Slab<LocalLump>,
    pub this_lump: LumpId,
}

#[impl_wasm_linker(module = "hearth::lump")]
impl LumpAbi {
    async fn this_lump(&self, memory: GuestMemory<'_>, ptr: u32) -> Result<()> {
        let id: &mut LumpId = memory.get_memory_ref(ptr)?;
        *id = self.this_lump;
        Ok(())
    }

    async fn from_id(&mut self, memory: GuestMemory<'_>, id_ptr: u32) -> Result<u32> {
        let id: LumpId = *memory.get_memory_ref(id_ptr)?;
        let bytes = self
            .lump_store
            .get_lump(&id)
            .await
            .ok_or_else(|| anyhow!("couldn't find {:?} in lump store", id))?;
        Ok(self.lump_handles.insert(LocalLump { id, bytes }) as u32)
    }

    async fn load(&mut self, memory: GuestMemory<'_>, ptr: u32, len: u32) -> Result<u32> {
        let bytes: Bytes = memory.get_slice(ptr, len)?.to_vec().into();
        let id = self.lump_store.add_lump(bytes.clone()).await;
        let lump = LocalLump { id, bytes };
        let handle = self.lump_handles.insert(lump) as u32;
        Ok(handle)
    }

    fn get_id(&self, memory: GuestMemory<'_>, handle: u32, id_ptr: u32) -> Result<()> {
        let lump = self.get_lump(handle)?;
        let id: &mut LumpId = memory.get_memory_ref(id_ptr)?;
        *id = lump.id;
        Ok(())
    }

    fn get_len(&self, handle: u32) -> Result<u32> {
        self.get_lump(handle).map(|lump| lump.bytes.len() as u32)
    }

    fn get_data(&self, memory: GuestMemory<'_>, handle: u32, ptr: u32) -> Result<()> {
        let lump = self.get_lump(handle)?;
        let len = lump.bytes.len() as u32;
        let dst = memory.get_slice(ptr, len)?;
        dst.copy_from_slice(&lump.bytes);
        Ok(())
    }

    fn free(&mut self, handle: u32) -> Result<()> {
        self.lump_handles
            .try_remove(handle as usize)
            .map(|_| ())
            .ok_or_else(|| anyhow!("lump handle {} is invalid", handle))
    }
}

impl LumpAbi {
    pub fn new(runtime: &Runtime, this_lump: LumpId) -> Self {
        Self {
            lump_store: runtime.lump_store.clone(),
            lump_handles: Default::default(),
            this_lump,
        }
    }

    fn get_lump(&self, handle: u32) -> Result<&LocalLump> {
        self.lump_handles
            .get(handle as usize)
            .ok_or_else(|| anyhow!("lump handle {} is invalid", handle))
    }
}

/// Implements the `hearth::process` ABI module.
pub struct ProcessAbi {
    pub ctx: Arc<Mutex<Process>>,
}

#[impl_wasm_linker(module = "hearth::process")]
impl ProcessAbi {
    async fn get_flags(&self, cap: u32) -> Result<u32> {
        self.ctx
            .lock()
            .await
            .get_capability_flags(cap as usize)
            .map(|flags| flags.bits())
    }

    async fn copy(&self, cap: u32, new_flags: u32) -> Result<u32> {
        let new_flags = Flags::from_bits(new_flags).context("flags have unrecognized bits set")?;

        self.ctx
            .lock()
            .await
            .make_capability(cap as usize, new_flags)
            .map(|cap| cap as u32)
    }

    async fn kill(&self, cap: u32) -> Result<()> {
        self.ctx.lock().await.kill(cap as usize)
    }

    async fn free(&self, cap: u32) -> Result<()> {
        self.ctx.lock().await.delete_capability(cap as usize)
    }
}

impl ProcessAbi {
    pub fn new(ctx: Arc<Mutex<Process>>) -> Self {
        Self { ctx }
    }
}

/// Implements the `hearth::service` ABI module.
pub struct ServiceAbi {
    pub ctx: Arc<Mutex<Process>>,
}

#[impl_wasm_linker(module = "hearth::service")]
impl ServiceAbi {
    async fn get(&self, memory: GuestMemory<'_>, name_ptr: u32, name_len: u32) -> Result<u32> {
        Ok(self
            .ctx
            .lock()
            .await
            .get_service(memory.get_str(name_ptr, name_len)?)
            .map(|handle| handle as u32)
            .unwrap_or(u32::MAX))
    }
}

impl ServiceAbi {
    pub fn new(ctx: Arc<Mutex<Process>>) -> Self {
        Self { ctx }
    }
}

/// Implements the `hearth::signal` ABI module.
pub struct SignalAbi {
    pub signal_store: Slab<ContextSignal>,
    pub ctx: Arc<Mutex<Process>>,
}

#[impl_wasm_linker(module = "hearth::signal")]
impl SignalAbi {
    async fn send(
        &mut self,
        memory: GuestMemory<'_>,
        dst_cap: u32,
        data_ptr: u32,
        data_len: u32,
        caps_ptr: u32,
        caps_num: u32,
    ) -> Result<()> {
        self.ctx.lock().await.send(
            dst_cap as usize,
            ContextMessage {
                data: memory.get_slice(data_ptr, data_len)?.to_vec(),
                caps: memory
                    .get_memory_slice::<u32>(caps_ptr, caps_num)?
                    .into_iter()
                    .map(|cap| *cap as usize)
                    .collect(),
            },
        )
    }

    async fn recv(&mut self) -> Result<u32> {
        match self.ctx.lock().await.recv().await {
            None => Err(anyhow!("process killed")),
            Some(signal) => Ok(self.signal_store.insert(signal) as u32),
        }
    }

    async fn recv_timeout(&mut self, timeout_us: u64) -> Result<u32> {
        let duration = std::time::Duration::from_micros(timeout_us);
        tokio::select! {
            result = self.recv() => result,
            _ = tokio::time::sleep(duration) => Ok(u32::MAX),
        }
    }

    fn get_data_len(&self, handle: u32) -> Result<u32> {
        Ok(match self.get_signal(handle)? {
            ContextSignal::Unlink { .. } => 0,
            ContextSignal::Message(ContextMessage { data, .. }) => data.len() as u32,
        })
    }

    fn get_data(&self, memory: GuestMemory<'_>, handle: u32, ptr: u32) -> Result<()> {
        match self.get_signal(handle)? {
            ContextSignal::Unlink { .. } => {
                bail!("cannot retrieve data of unlink signal {}", handle);
            }
            ContextSignal::Message(ContextMessage { data, .. }) => {
                let len = data.len() as u32;
                let dst = memory.get_slice(ptr, len)?;
                dst.copy_from_slice(data.as_slice());
                Ok(())
            }
        }
    }

    fn get_caps_num(&self, handle: u32) -> Result<u32> {
        Ok(match self.get_signal(handle)? {
            ContextSignal::Unlink { .. } => 1,
            ContextSignal::Message(ContextMessage { caps, .. }) => caps.len() as u32,
        })
    }

    fn get_caps(&self, memory: GuestMemory<'_>, handle: u32, ptr: u32) -> Result<()> {
        let caps = match self.get_signal(handle)? {
            ContextSignal::Unlink { subject } => vec![*subject as u32],
            ContextSignal::Message(ContextMessage { caps, .. }) => {
                caps.iter().map(|cap| *cap as u32).collect()
            }
        };

        let len = caps.len() as u32;
        let dst = memory.get_memory_slice::<u32>(ptr, len)?;
        dst.copy_from_slice(caps.as_slice());
        Ok(())
    }

    fn free(&mut self, handle: u32) -> Result<()> {
        self.signal_store
            .try_remove(handle as usize)
            .map(|_| ())
            .ok_or_else(|| anyhow!("signal handle {} is invalid", handle))
    }
}

impl SignalAbi {
    pub fn new(ctx: Arc<Mutex<Process>>) -> Self {
        Self {
            signal_store: Slab::new(),
            ctx,
        }
    }

    fn get_signal(&self, handle: u32) -> Result<&ContextSignal> {
        self.signal_store
            .get(handle as usize)
            .with_context(|| format!("signal handle {} is invalid", handle))
    }
}

/// This contains all script-accessible process-related stuff.
pub struct ProcessData {
    pub log: LogAbi,
    pub lump: LumpAbi,
    pub process: ProcessAbi,
    pub service: ServiceAbi,
    pub signal: SignalAbi,
}

impl ProcessData {
    pub fn new(runtime: &Runtime, ctx: Process, this_lump: LumpId) -> Self {
        let ctx = Arc::new(Mutex::new(ctx));

        Self {
            log: LogAbi::new(ctx.to_owned()),
            lump: LumpAbi::new(runtime, this_lump),
            process: ProcessAbi::new(ctx.to_owned()),
            service: ServiceAbi::new(ctx.to_owned()),
            signal: SignalAbi::new(ctx),
        }
    }
}

macro_rules! impl_asmut {
    ($ty: ident, $sub_ty: ident, $sub_field: ident) => {
        impl ::std::convert::AsMut<$sub_ty> for $ty {
            fn as_mut(&mut self) -> &mut $sub_ty {
                &mut self.$sub_field
            }
        }
    };
}

impl_asmut!(ProcessData, LogAbi, log);
impl_asmut!(ProcessData, LumpAbi, lump);
impl_asmut!(ProcessData, ProcessAbi, process);
impl_asmut!(ProcessData, ServiceAbi, service);
impl_asmut!(ProcessData, SignalAbi, signal);

impl ProcessData {
    /// Adds all module ABIs to the given linker.
    pub fn add_to_linker(linker: &mut Linker<Self>) {
        LogAbi::add_to_linker(linker);
        LumpAbi::add_to_linker(linker);
        ProcessAbi::add_to_linker(linker);
        ServiceAbi::add_to_linker(linker);
        SignalAbi::add_to_linker(linker);
    }
}

struct WasmProcess {
    engine: Arc<Engine>,
    linker: Arc<Linker<ProcessData>>,
    module: Arc<Module>,
    this_lump: LumpId,
    entrypoint: Option<u32>,
}

impl WasmProcess {
    async fn run(&mut self, runtime: Arc<Runtime>, ctx: Process) {
        let pid = ctx.get_pid();
        match self
            .run_inner(runtime, ctx)
            .await
            .with_context(|| format!("error in Wasm process {}", pid))
        {
            Ok(()) => {}
            Err(err) => {
                error!("{:?}", err);
            }
        }
    }

    async fn run_inner(&mut self, runtime: Arc<Runtime>, ctx: Process) -> Result<()> {
        // TODO log using the process log instead of tracing?
        let data = ProcessData::new(runtime.as_ref(), ctx, self.this_lump);
        let mut store = Store::new(&self.engine, data);
        store.epoch_deadline_async_yield_and_update(1);
        let instance = self
            .linker
            .instantiate_async(&mut store, &self.module)
            .await
            .context("instantiating Wasm instance")?;

        if let Some(entrypoint) = self.entrypoint {
            let cb = instance
                .get_typed_func::<u32, ()>(&mut store, "_hearth_spawn_by_index")
                .context("lookup _hearth_spawn_by_index")?;
            cb.call_async(&mut store, entrypoint)
                .await
                .context("calling Wasm entrypoint")?;
        } else {
            let cb = instance.get_typed_func::<(), ()>(&mut store, "run")?;
            cb.call_async(&mut store, ())
                .await
                .context("calling Wasm run()")?;
        }

        Ok(())
    }
}

pub struct WasmProcessSpawner {
    engine: Arc<Engine>,
    linker: Arc<Linker<ProcessData>>,
}

impl WasmProcessSpawner {
    async fn run(self, runtime: Arc<Runtime>, mut ctx: Process) {
        debug!("Listening to Wasm spawn requests");
        while let Some(signal) = ctx.recv().await {
            let ContextSignal::Message(ContextMessage{ data: msg_data, caps: msg_caps }) = signal else {
                // TODO make this a process log
                warn!("Wasm spawner expected message but received: {:?}", signal);
                continue;
            };

            let Some(parent) = msg_caps.get(0).copied() else {
                // TODO make this a process log
                debug!("Spawn request has no return address");
                continue;
            };

            let message: WasmSpawnInfo = match serde_json::from_slice(&msg_data) {
                Ok(message) => message,
                Err(err) => {
                    ctx.log(ProcessLogEvent {
                        level: ProcessLogLevel::Error,
                        module: "WasmProcessSpawner".to_string(),
                        content: format!("Failed to parse WasmSpawnInfo: {:?}", err),
                    });

                    warn!("Failed to parse WasmSpawnInfo: {:?}", err);

                    continue;
                }
            };

            debug!("Spawning Wasm module lump {}", message.lump);

            match runtime
                .asset_store
                .load_asset::<WasmModuleLoader>(&message.lump)
                .await
            {
                Err(err) => {
                    ctx.log(ProcessLogEvent {
                        level: ProcessLogLevel::Error,
                        module: "WasmProcessSpawner".to_string(),
                        content: format!("Failed to load Wasm module: {:?}", err),
                    });

                    warn!("Failed to load Wasm module {}: {:?}", message.lump, err);
                }
                Ok(module) => {
                    debug!("Spawning module {}", message.lump);
                    let info = ProcessInfo {};
                    let flags = Flags::SEND | Flags::KILL;
                    let child = runtime.process_factory.spawn(info, flags);
                    let child_cap = ctx.copy_self_capability(&child);
                    let result = ctx.send(
                        parent,
                        ContextMessage {
                            data: vec![],
                            caps: vec![child_cap],
                        },
                    );

                    // TODO make run_inner to catch errors safely
                    ctx.delete_capability(child_cap).unwrap();

                    let runtime = runtime.to_owned();
                    let engine = self.engine.to_owned();
                    let linker = self.linker.to_owned();
                    tokio::spawn(async move {
                        WasmProcess {
                            engine,
                            linker,
                            module,
                            entrypoint: message.entrypoint,
                            this_lump: message.lump,
                        }
                        .run(runtime, child)
                        .await;
                    });

                    match result {
                        Ok(()) => {}
                        Err(err) => error!("Replying child cap to parent error: {:?}", err),
                    }
                }
            }
        }
    }
}

pub struct WasmModuleLoader {
    engine: Arc<Engine>,
}

#[async_trait]
impl AssetLoader for WasmModuleLoader {
    type Asset = Module;

    async fn load_asset(&self, data: &[u8]) -> anyhow::Result<Module> {
        Module::new(&self.engine, data)
    }
}

pub struct WasmPlugin {
    engine: Arc<Engine>,
    asset_store_tx: Vec<oneshot::Sender<Arc<AssetStore>>>,
}

#[async_trait]
impl Plugin for WasmPlugin {
    fn build(&mut self, builder: &mut RuntimeBuilder) {
        let mut linker = Linker::new(&self.engine);
        ProcessData::add_to_linker(&mut linker);

        let spawner = WasmProcessSpawner {
            engine: self.engine.to_owned(),
            linker: Arc::new(linker),
        };

        builder.add_service(
            "hearth.cognito.WasmProcessSpawner".into(),
            ProcessInfo {},
            Flags::SEND,
            |runtime, process| {
                tokio::spawn(async move {
                    spawner.run(runtime, process).await;
                });
            },
        );

        builder.add_asset_loader(WasmModuleLoader {
            engine: self.engine.to_owned(),
        });
    }

    async fn run(&mut self, runtime: Arc<Runtime>) {
        for tx in self.asset_store_tx.drain(..) {
            let _ = tx.send(runtime.asset_store.to_owned());
        }

        // TODO make this time slice duration configurable
        let duration = std::time::Duration::from_micros(100);
        loop {
            tokio::time::sleep(duration).await;
            self.engine.increment_epoch();
        }
    }
}

impl WasmPlugin {
    pub fn new() -> Self {
        let mut config = Config::new();
        config.async_support(true);
        config.epoch_interruption(true);
        config.memory_init_cow(true);

        let engine = Engine::new(&config).unwrap();

        Self {
            engine: Arc::new(engine),
            asset_store_tx: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link() {
        let mut config = Config::new();
        config.async_support(true);
        let engine = Engine::new(&config).unwrap();
        let mut linker = Linker::new(&engine);
        ProcessData::add_to_linker(&mut linker);
    }
}

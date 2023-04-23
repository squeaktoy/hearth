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
use hearth_core::process::{Message, Process, ProcessContext};
use hearth_core::runtime::{Plugin, Runtime, RuntimeBuilder};
use hearth_core::tokio;
use hearth_macros::impl_wasm_linker;
use hearth_rpc::hearth_types::wasm::WasmSpawnInfo;
use hearth_rpc::hearth_types::{LumpId, PeerId, ProcessId, ProcessLogLevel};
use hearth_rpc::{remoc, ProcessInfo, ProcessLogEvent, ProcessStore};
use hearth_wasm::{GuestMemory, WasmLinker};
use remoc::rtc::async_trait;
use slab::Slab;
use tokio::sync::{oneshot, Mutex};
use tracing::{debug, error, warn};
use wasmtime::*;

/// Implements the `hearth::asset` ABI module.
#[derive(Debug, Default)]
pub struct AssetAbi {}

#[impl_wasm_linker(module = "hearth::asset")]
impl AssetAbi {}

/// Implements the `hearth::log` ABI module.
pub struct LogAbi {
    pub ctx: Arc<Mutex<ProcessContext>>,
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
    pub fn new(ctx: Arc<Mutex<ProcessContext>>) -> Self {
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
#[derive(Debug, Default)]
pub struct LumpAbi {
    pub lump_store: Arc<LumpStoreImpl>,
    pub lump_handles: Slab<LocalLump>,
}

#[impl_wasm_linker(module = "hearth::lump")]
impl LumpAbi {
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
        let bytes: Bytes = memory
            .get_slice(ptr as usize, len as usize)?
            .to_vec()
            .into();
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
        let len = lump.bytes.len();
        let dst = memory.get_slice(ptr as usize, len)?;
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
    fn get_lump(&self, handle: u32) -> Result<&LocalLump> {
        self.lump_handles
            .get(handle as usize)
            .ok_or_else(|| anyhow!("lump handle {} is invalid", handle))
    }
}

/// Implements the `hearth::message` ABI module.
pub struct MessageAbi {
    pub msg_store: Slab<Message>,
    pub ctx: Arc<Mutex<ProcessContext>>,
}

#[impl_wasm_linker(module = "hearth::message")]
impl MessageAbi {
    async fn recv(&mut self) -> Result<u32> {
        match self.ctx.lock().await.recv().await {
            None => Err(anyhow!("Process killed")),
            Some(msg) => Ok(self.msg_store.insert(msg) as u32),
        }
    }

    async fn recv_timeout(&mut self, timeout_us: u64) -> Result<u32> {
        let duration = std::time::Duration::from_micros(timeout_us);
        tokio::select! {
            result = self.recv() => result,
            _ = tokio::time::sleep(duration) => Ok(u32::MAX),
        }
    }

    async fn send(&mut self, memory: GuestMemory<'_>, pid: u64, ptr: u32, len: u32) -> Result<()> {
        let data = memory.get_slice(ptr as usize, len as usize)?;
        let data = data.to_vec();
        let pid = ProcessId(pid);
        self.ctx.lock().await.send_message(pid, data).await?;
        Ok(())
    }

    async fn get_sender(&self, handle: u32) -> Result<u64> {
        self.get_msg(handle).map(|msg| msg.sender.0)
    }

    async fn get_len(&self, handle: u32) -> Result<u32> {
        self.get_msg(handle).map(|msg| msg.data.len() as u32)
    }

    async fn get_data(&self, memory: GuestMemory<'_>, handle: u32, ptr: u32) -> Result<()> {
        let msg = self.get_msg(handle)?;
        let len = msg.data.len();
        let dst = memory.get_slice(ptr as usize, len)?;
        dst.copy_from_slice(msg.data.as_slice());
        Ok(())
    }

    async fn free(&mut self, handle: u32) -> Result<()> {
        self.msg_store
            .try_remove(handle as usize)
            .map(|_| ())
            .ok_or_else(|| anyhow!("message handle {} is invalid", handle))
    }
}

impl MessageAbi {
    pub fn new(ctx: Arc<Mutex<ProcessContext>>) -> Self {
        Self {
            msg_store: Slab::new(),
            ctx,
        }
    }

    fn get_msg(&self, handle: u32) -> Result<&Message> {
        self.msg_store
            .get(handle as usize)
            .with_context(|| format!("message handle {} is invalid", handle))
    }
}

/// Implements the `hearth::process` ABI module.
pub struct ProcessAbi {
    pub ctx: Arc<Mutex<ProcessContext>>,
    pub this_lump: LumpId,
}

#[impl_wasm_linker(module = "hearth::process")]
impl ProcessAbi {
    async fn this_lump(&self, memory: GuestMemory<'_>, ptr: u32) -> Result<()> {
        let id: &mut LumpId = memory.get_memory_ref(ptr)?;
        *id = self.this_lump;
        Ok(())
    }

    async fn this_pid(&self) -> u64 {
        self.ctx.lock().await.get_pid().0
    }

    async fn kill(&self, _pid: u64) -> Result<()> {
        Err(anyhow!("killing other processes is unimplemented"))
    }
}

impl ProcessAbi {
    pub fn new(ctx: Arc<Mutex<ProcessContext>>, this_lump: LumpId) -> Self {
        Self { ctx, this_lump }
    }
}

/// Implements the `hearth::service` ABI module.
pub struct ServiceAbi {
    pub ctx: Arc<Mutex<ProcessContext>>,
}

#[impl_wasm_linker(module = "hearth::service")]
impl ServiceAbi {
    async fn lookup(
        &self,
        memory: GuestMemory<'_>,
        peer: u32,
        name_ptr: u32,
        name_len: u32,
    ) -> Result<u64> {
        let name = memory.get_str(name_ptr, name_len)?.to_string();

        let ctx = self.ctx.lock().await;
        if peer != ctx.get_pid().split().0 .0 {
            bail!("registry operations on remote peers are unimplemented");
        }

        let services = ctx
            .get_process_store()
            .follow_service_list()
            .await?
            .take_initial()
            .context("could not take initial service list")?;

        services
            .get(&name)
            .map(|pid| ProcessId::from_peer_process(PeerId(peer), *pid).0)
            .with_context(|| format!("could not lookup service {:?}", name))
    }

    async fn register(
        &self,
        memory: GuestMemory<'_>,
        pid: u64,
        name_ptr: u32,
        name_len: u32,
    ) -> Result<()> {
        let pid = ProcessId(pid);
        let name = memory.get_str(name_ptr, name_len)?.to_string();

        let ctx = self.ctx.lock().await;
        if pid.split().0 != ctx.get_pid().split().0 {
            bail!("registry operations on remote peers are unimplemented");
        }

        ctx.get_process_store()
            .register_service(pid.split().1, name)
            .await?;

        Ok(())
    }

    async fn deregister(
        &self,
        memory: GuestMemory<'_>,
        peer: u32,
        name_ptr: u32,
        name_len: u32,
    ) -> Result<()> {
        let name = memory.get_str(name_ptr, name_len)?.to_string();

        let ctx = self.ctx.lock().await;
        if peer != ctx.get_pid().split().0 .0 {
            bail!("registry operations on remote peers are unimplemented");
        }

        ctx.get_process_store().deregister_service(name).await?;

        Ok(())
    }
}

impl ServiceAbi {
    pub fn new(ctx: Arc<Mutex<ProcessContext>>) -> Self {
        Self { ctx }
    }
}

/// This contains all script-accessible process-related stuff.
pub struct ProcessData {
    pub asset: AssetAbi,
    pub log: LogAbi,
    pub lump: LumpAbi,
    pub message: MessageAbi,
    pub process: ProcessAbi,
    pub service: ServiceAbi,
}

impl ProcessData {
    pub fn new(ctx: ProcessContext, this_lump: LumpId) -> Self {
        let ctx = Arc::new(Mutex::new(ctx));

        Self {
            asset: Default::default(),
            log: LogAbi::new(ctx.to_owned()),
            lump: Default::default(),
            message: MessageAbi::new(ctx.to_owned()),
            process: ProcessAbi::new(ctx.to_owned(), this_lump),
            service: ServiceAbi::new(ctx),
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

impl_asmut!(ProcessData, AssetAbi, asset);
impl_asmut!(ProcessData, LogAbi, log);
impl_asmut!(ProcessData, LumpAbi, lump);
impl_asmut!(ProcessData, MessageAbi, message);
impl_asmut!(ProcessData, ProcessAbi, process);
impl_asmut!(ProcessData, ServiceAbi, service);

impl ProcessData {
    /// Adds all module ABIs to the given linker.
    pub fn add_to_linker(linker: &mut Linker<Self>) {
        AssetAbi::add_to_linker(linker);
        LogAbi::add_to_linker(linker);
        LumpAbi::add_to_linker(linker);
        MessageAbi::add_to_linker(linker);
        ProcessAbi::add_to_linker(linker);
        ServiceAbi::add_to_linker(linker);
    }
}

struct WasmProcess {
    engine: Arc<Engine>,
    linker: Arc<Linker<ProcessData>>,
    module: Arc<Module>,
    this_lump: LumpId,
    entrypoint: Option<u32>,
}

#[async_trait]
impl Process for WasmProcess {
    fn get_info(&self) -> ProcessInfo {
        ProcessInfo {}
    }

    async fn run(&mut self, ctx: ProcessContext) {
        let pid = ctx.get_pid();
        match self
            .run_inner(ctx)
            .await
            .with_context(|| format!("error in Wasm process {}", pid))
        {
            Ok(()) => {}
            Err(err) => {
                error!("{:?}", err);
            }
        }
    }
}

impl WasmProcess {
    async fn run_inner(&mut self, ctx: ProcessContext) -> Result<()> {
        // TODO log using the process log instead of tracing?
        let data = ProcessData::new(ctx, self.this_lump);
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
    asset_store: oneshot::Receiver<Arc<AssetStore>>,
}

#[async_trait]
impl Process for WasmProcessSpawner {
    fn get_info(&self) -> ProcessInfo {
        ProcessInfo {}
    }

    async fn run(&mut self, mut ctx: ProcessContext) {
        let asset_store = oneshot::channel().1;
        let asset_store = std::mem::replace(&mut self.asset_store, asset_store)
            .await
            .expect("asset store sender dropped");

        debug!("Listening to Wasm spawn requests");
        while let Some(message) = ctx.recv().await {
            let sender = message.sender;
            debug!("Received message from {:?}", sender.split());

            let message: WasmSpawnInfo = match serde_json::from_slice(&message.data) {
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

            match asset_store
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
                    let pid = ctx
                        .get_process_store()
                        .spawn(WasmProcess {
                            engine: self.engine.to_owned(),
                            linker: self.linker.to_owned(),
                            module,
                            entrypoint: message.entrypoint,
                            this_lump: message.lump,
                        })
                        .await;

                    debug!("Spawned PID: {:?}", pid);
                    let _ = ctx
                        .send_message(sender, format!("{}", pid.0).into_bytes())
                        .await;
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

        let (asset_store_tx, asset_store) = oneshot::channel();
        self.asset_store_tx.push(asset_store_tx);

        builder.add_service(
            "hearth.cognito.WasmProcessSpawner".into(),
            WasmProcessSpawner {
                engine: self.engine.to_owned(),
                linker: Arc::new(linker),
                asset_store,
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

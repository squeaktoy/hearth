use std::sync::Arc;

use hearth_core::{
    async_trait,
    runtime::{RuntimeBuilder, RuntimeConfig},
};
use hearth_rpc::{
    hearth_types::{wasm::WasmSpawnInfo, Flags, PeerId},
    remoc::{self, robs::hash_map::HashMapSubscription, rtc::ServerShared},
    CallResult, PeerApiClient, PeerInfo, PeerProvider, PeerProviderServerShared, ResourceError,
    ResourceResult,
};
use tracing::info;

#[tokio::main]
async fn main() {
    hearth_core::init_logging();

    let wasm_path = std::env::args()
        .nth(1)
        .expect("expected path to .wasm file");
    let wasm_data = std::fs::read(wasm_path).unwrap();

    let peer_provider = MockPeerProvider;
    let (peer_provider_server, peer_provider) =
        PeerProviderServerShared::<_, remoc::codec::Default>::new(Arc::new(peer_provider), 1024);
    tokio::spawn(async move {
        peer_provider_server.serve(true).await;
    });

    let config = RuntimeConfig {
        peer_provider,
        this_peer: PeerId(0),
        info: PeerInfo { nickname: None },
    };

    let config_path = hearth_core::get_config_path();
    let config_file = hearth_core::load_config(&config_path).unwrap();
    let mut builder = RuntimeBuilder::new(config_file);
    builder.add_plugin(hearth_cognito::WasmPlugin::new());
    let (runtime, join_handles) = builder.run(config);

    let wasm_lump = runtime.lump_store.add_lump(wasm_data.into()).await;
    let spawn_info = WasmSpawnInfo {
        lump: wasm_lump,
        entrypoint: None,
    };

    let mut parent = runtime
        .process_factory
        .spawn(hearth_rpc::ProcessInfo {}, Flags::SEND);

    // TODO block RuntimeBuilder::run() until after all services are registered
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let wasm_spawner = parent
        .get_service("hearth.cognito.WasmProcessSpawner")
        .expect("Wasm spawner service not found");

    parent
        .send(
            wasm_spawner,
            hearth_core::process::context::ContextMessage::Data {
                data: serde_json::to_vec(&spawn_info).unwrap(),
                caps: vec![0],
            },
        )
        .unwrap();

    hearth_core::wait_for_interrupt().await;

    info!("Interrupt received; exiting runtime");
    for join in join_handles {
        join.abort();
    }
}

struct MockPeerProvider;

#[async_trait]
impl PeerProvider for MockPeerProvider {
    async fn find_peer(&self, _id: PeerId) -> ResourceResult<PeerApiClient> {
        Err(ResourceError::Unavailable)
    }

    async fn follow_peer_list(&self) -> CallResult<HashMapSubscription<PeerId, PeerInfo>> {
        Err(remoc::rtc::CallError::RemoteForward)
    }
}

use hearth_core::{
    process::factory::ProcessInfo,
    runtime::{RuntimeBuilder, RuntimeConfig},
};
use hearth_types::{wasm::WasmSpawnInfo, Flags, PeerId};
use tracing::info;

#[tokio::main]
async fn main() {
    hearth_core::init_logging();

    let wasm_path = std::env::args()
        .nth(1)
        .expect("expected path to .wasm file");
    let wasm_data = std::fs::read(wasm_path).unwrap();

    let config = RuntimeConfig {
        this_peer: PeerId(0),
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

    let mut parent = runtime.process_factory.spawn(ProcessInfo {}, Flags::SEND);

    // TODO block RuntimeBuilder::run() until after all services are registered
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let wasm_spawner = parent
        .get_service("hearth.cognito.WasmProcessSpawner")
        .expect("Wasm spawner service not found");

    parent
        .send(
            wasm_spawner,
            hearth_core::process::context::ContextMessage {
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

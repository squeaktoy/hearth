use hearth_core::{
    flue::{ContextSignal, Permissions},
    process::ProcessInfo,
    runtime::{RuntimeBuilder, RuntimeConfig},
};
use hearth_types::{registry::RegistryRequest, wasm::WasmSpawnInfo};
use tracing::info;

#[tokio::main]
async fn main() {
    hearth_core::init_logging();

    let wasm_path = std::env::args()
        .nth(1)
        .expect("expected path to .wasm file");
    let wasm_data = std::fs::read(wasm_path).unwrap();

    let config = RuntimeConfig {};

    let config_path = hearth_core::get_config_path();
    let config_file = hearth_core::load_config(&config_path).unwrap();
    let mut builder = RuntimeBuilder::new(config_file);
    builder.add_plugin(hearth_cognito::WasmPlugin::new());
    let runtime = builder.run(config).await;

    let wasm_lump = runtime.lump_store.add_lump(wasm_data.into()).await;
    let spawn_info = WasmSpawnInfo {
        lump: wasm_lump,
        entrypoint: None,
    };

    let parent = runtime.process_factory.spawn(ProcessInfo {});
    let response = parent.borrow_store().create_mailbox().unwrap();
    let response_cap = response.make_capability(Permissions::SEND);

    let registry = runtime.registry.borrow_parent();
    let registry = parent.borrow_table().import(registry, Permissions::SEND);
    let registry = parent.borrow_table().wrap_handle(registry).unwrap();

    let request = RegistryRequest::Get {
        name: "hearth.cognito.WasmProcessSpawner".to_string(),
    };

    registry
        .send(
            &serde_json::to_vec(&request).unwrap(),
            &[&response_cap, &registry],
        )
        .await
        .unwrap();

    let spawner = response
        .recv(|signal| {
            let ContextSignal::Message { mut caps, .. } = signal else {
                panic!("expected message, got {:?}", signal);
            };

            caps.remove(0)
        })
        .await
        .unwrap();

    let spawner = parent.borrow_table().wrap_handle(spawner).unwrap();

    spawner
        .send(&serde_json::to_vec(&spawn_info).unwrap(), &[&response_cap])
        .await
        .unwrap();

    hearth_core::wait_for_interrupt().await;

    info!("Interrupt received; exiting runtime");
}

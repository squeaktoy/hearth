#[no_mangle]
pub extern "C" fn run() {
    let this_peer = hearth_guest::this_pid().split().0;
    let spawner = hearth_guest::service_lookup(this_peer, "hearth.cognito.WasmProcessSpawner")
        .expect("Couldn't find Wasm spawner service");
    hearth_guest::send(spawner, b"test message");
}

use tracing::{debug, error, info, Level};
use tracing_subscriber::prelude::*;

/// Implementations of the `hearth-rpc` crate's RPC interfaces.
pub mod api;

/// Helper function to set up console logging with reasonable defaults.
pub fn init_logging() {
    let filter = tracing_subscriber::filter::Targets::new()
        .with_target("wgpu", Level::INFO)
        .with_target("wgpu_core", Level::WARN)
        .with_target("wgpu_hal", Level::WARN)
        .with_default(Level::DEBUG);

    let format = tracing_subscriber::fmt::layer().compact();

    tracing_subscriber::registry()
        .with(filter)
        .with(format)
        .init();
}

/// Helper function to wait for Ctrl+C with nice logging.
pub async fn wait_for_interrupt() {
    debug!("Waiting for interrupt signal");
    match tokio::signal::ctrl_c().await {
        Ok(()) => info!("Interrupt signal received"),
        Err(err) => error!("Interrupt await error: {:?}", err),
    }
}

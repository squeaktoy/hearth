/// Implementations of the `hearth-rpc` crate's RPC interfaces.
pub mod api;

/// Helper function to set up console logging with reasonable defaults.
pub fn init_logging() {
    let format = tracing_subscriber::fmt::format().compact();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .event_format(format)
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

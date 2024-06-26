use std::path::{Path, PathBuf};

pub use async_trait::async_trait;
use tracing::{debug, error, info, Level};
use tracing_subscriber::prelude::*;

pub use anyhow;
pub use flue;
pub use hearth_macros;
pub use hearth_schema;
pub use tokio;
pub use tracing;

/// Asset loading and storage.
pub mod asset;

/// Network connection.
pub mod connection;

/// Lump loading and storage.
pub mod lump;

/// Local process spawning and management.
pub mod process;

/// The native registry implementation.
pub mod registry;

/// Peer runtime building and execution.
pub mod runtime;

/// Utilities for host-side runtime management.
pub mod utils;

/// Helper function to set up console logging with reasonable defaults.
pub fn init_logging() {
    let filter = tracing_subscriber::filter::Targets::new()
        .with_target("wgpu", Level::INFO)
        .with_target("wgpu_core", Level::WARN)
        .with_target("wgpu_hal", Level::WARN)
        .with_target("hearth", Level::DEBUG)
        .with_default(Level::INFO);

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

/// Gets the system directory for Hearth configuration files.
///
/// Panics if something fails for whatever reason.
pub fn get_config_dir() -> PathBuf {
    directories::ProjectDirs::from("rs", "hearth", "hearth")
        .expect("Failed to get Hearth project directories")
        .config_dir()
        .to_owned()
}

/// Gets the default path of the main Hearth configuration file.
///
/// Panics if something fails for whatever reason.
pub fn get_config_path() -> PathBuf {
    get_config_dir().join("config.toml")
}

/// Loads a configuration file from the given path.
pub fn load_config(path: &Path) -> anyhow::Result<toml::Table> {
    info!("Loading configuration file from {:?}", path);
    let config = std::fs::read_to_string(path)
        .map_err(|err| anyhow::anyhow!("Failed to load config file at {:?}: {:?}", path, err))?;
    toml::from_str(&config)
        .map_err(|err| anyhow::anyhow!("Failed to deserialize config: {:?}", err))
}

use clap::{Parser, Subcommand};

/// Command-line interface (CLI) for interacting with a Hearth daemon over IPC.
#[derive(Debug, Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {}

fn main() {
    let args = Args::parse();

    println!("Hello, world!");
}

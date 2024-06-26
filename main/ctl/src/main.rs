use std::{collections::HashMap, fmt::Display, process::ExitCode};

use clap::{Parser, Subcommand};
use hearth_ipc::Connection;

pub const EX_PROTOCOL: u8 = 76;

pub struct DaemonOffer {}

pub struct CommandError {
    message: String,
    exit_code: u8,
}

trait ToCommandError<T, E> {
    fn to_command_error<C: Display>(self, context: C, exit_code: u8) -> Result<T, CommandError>;
}

impl<T, E> ToCommandError<T, E> for Result<T, E>
where
    E: Display,
{
    fn to_command_error<C: Display>(self, context: C, exit_code: u8) -> Result<T, CommandError> {
        match self {
            Ok(ok) => Ok(ok),
            Err(e) => Err(CommandError {
                message: format!("{}: {}", context, e),
                exit_code,
            }),
        }
    }
}

impl<T> ToCommandError<T, ()> for Option<T> {
    fn to_command_error<C: Display>(self, context: C, exit_code: u8) -> Result<T, CommandError> {
        match self {
            Some(val) => Ok(val),
            None => Err(CommandError {
                message: context.to_string(),
                exit_code,
            }),
        }
    }
}

pub type CommandResult<T> = Result<T, CommandError>;

/// Command-line interface (CLI) for interacting with a Hearth daemon over IPC.
#[derive(Debug, Parser)]
pub struct Args {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// A dummy command.
    Dummy,
}

impl Commands {
    pub async fn run(self) -> CommandResult<()> {
        Ok(())
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let args = Args::parse();

    match args.command.run().await {
        Ok(_) => 0,
        Err(e) => {
            eprintln!("ERROR: {}", e.message);
            e.exit_code
        }
    }
    .into()
}

async fn get_daemon() -> CommandResult<Connection> {
    hearth_ipc::connect()
        .await
        .to_command_error("connecting to Hearth daemon", EX_PROTOCOL)
}

fn hash_map_to_ordered_vec<K: Copy + Ord, V>(map: HashMap<K, V>) -> Vec<(K, V)> {
    let mut vec = map.into_iter().collect::<Vec<(K, V)>>();
    vec.sort_by_cached_key(|k| k.0);
    vec
}

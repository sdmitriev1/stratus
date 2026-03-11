mod dump;
mod status;

use clap::{Parser, Subcommand};

pub mod proto {
    tonic::include_proto!("stratus.v1");
}

const DEFAULT_SOCKET: &str = "/run/stratus/stratusd.sock";

#[derive(Parser)]
#[command(name = "stratus", version, about = "Stratus VM orchestrator")]
struct Cli {
    /// Path to daemon Unix socket
    #[arg(long, default_value = DEFAULT_SOCKET, global = true)]
    socket: String,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Show daemon status
    Status,
    /// Dump all resources in the store
    Dump,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Status => status::run(&cli.socket).await?,
        Command::Dump => dump::run(&cli.socket).await?,
    }

    Ok(())
}

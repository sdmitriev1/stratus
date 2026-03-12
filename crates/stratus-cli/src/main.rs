mod apply;
mod connect;
mod delete;
mod dump;
mod get;
mod instance;
mod output;
mod status;

use anyhow::bail;
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
    /// Apply resources from YAML file(s)
    Apply {
        /// YAML file, directory, or "-" for stdin
        #[arg(short, long)]
        file: String,
    },
    /// Get resources by kind
    Get {
        /// Resource kind (e.g. network, instance, sg)
        kind: String,
        /// Optional resource name
        name: Option<String>,
        /// Output format
        #[arg(short, long, default_value = "table")]
        output: output::OutputFormat,
    },
    /// Delete a resource
    Delete {
        /// Resource kind
        kind: Option<String>,
        /// Resource name
        name: Option<String>,
        /// Delete resources from YAML file
        #[arg(short, long)]
        file: Option<String>,
    },
    /// Manage instances
    Instance {
        #[command(subcommand)]
        action: InstanceCommand,
    },
}

#[derive(Subcommand)]
enum InstanceCommand {
    /// Start an instance
    Start {
        /// Instance name
        name: String,
    },
    /// Stop an instance gracefully
    Stop {
        /// Instance name
        name: String,
    },
    /// Kill an instance immediately
    Kill {
        /// Instance name
        name: String,
    },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Status => status::run(&cli.socket).await?,
        Command::Dump => dump::run(&cli.socket).await?,
        Command::Apply { file } => apply::run(&cli.socket, &file).await?,
        Command::Get { kind, name, output } => {
            get::run(&cli.socket, &kind, name.as_deref(), output).await?
        }
        Command::Delete { kind, name, file } => match (kind, name, file) {
            (_, _, Some(file)) => delete::run_from_file(&cli.socket, &file).await?,
            (Some(kind), Some(name), None) => {
                delete::run_by_name(&cli.socket, &kind, &name).await?
            }
            _ => bail!("usage: stratus delete <kind> <name> or stratus delete -f <file>"),
        },
        Command::Instance { action } => match action {
            InstanceCommand::Start { name } => instance::start(&cli.socket, &name).await?,
            InstanceCommand::Stop { name } => instance::stop(&cli.socket, &name).await?,
            InstanceCommand::Kill { name } => instance::kill(&cli.socket, &name).await?,
        },
    }

    Ok(())
}

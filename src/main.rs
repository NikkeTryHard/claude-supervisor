//! Claude Supervisor - Automated Claude Code with AI oversight.

use clap::{Parser, Subcommand};
use tracing_subscriber::{EnvFilter, fmt, prelude::*};

use claude_supervisor::config::SupervisorConfig;
use claude_supervisor::supervisor::PolicyLevel;

#[derive(Parser)]
#[command(
    name = "claude-supervisor",
    about = "Automated Claude Code with AI oversight",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run Claude Code with supervision.
    Run {
        /// The task to execute.
        task: String,
        /// Policy level (permissive, moderate, strict).
        #[arg(short, long, default_value = "permissive")]
        policy: String,
        /// Auto-continue without user prompts.
        #[arg(long)]
        auto_continue: bool,
    },
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
}

fn parse_policy(s: &str) -> PolicyLevel {
    match s.to_lowercase().as_str() {
        "strict" => PolicyLevel::Strict,
        "moderate" => PolicyLevel::Moderate,
        _ => PolicyLevel::Permissive,
    }
}

#[tokio::main]
async fn main() {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            task,
            policy,
            auto_continue,
        } => {
            let config = SupervisorConfig {
                policy: parse_policy(&policy),
                auto_continue,
                ..Default::default()
            };
            tracing::info!(
                task = %task,
                policy = ?config.policy,
                auto_continue = config.auto_continue,
                "Starting Claude supervisor"
            );
            tracing::warn!("Supervisor not yet implemented");
        }
    }
}

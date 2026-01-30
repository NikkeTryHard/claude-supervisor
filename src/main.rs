//! Claude Supervisor - Automated Claude Code with AI oversight.

use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use claude_supervisor::config::SupervisorConfig;
use claude_supervisor::supervisor::PolicyLevel;

#[derive(Debug, Clone, Copy, ValueEnum)]
enum PolicyArg {
    Permissive,
    Moderate,
    Strict,
}

impl From<PolicyArg> for PolicyLevel {
    fn from(arg: PolicyArg) -> Self {
        match arg {
            PolicyArg::Permissive => PolicyLevel::Permissive,
            PolicyArg::Moderate => PolicyLevel::Moderate,
            PolicyArg::Strict => PolicyLevel::Strict,
        }
    }
}

#[derive(Parser)]
#[command(
    name = "claude-supervisor",
    about = "Automated Claude Code with AI oversight",
    version
)]
struct Cli {
    /// Increase verbosity (-v, -vv, -vvv)
    #[arg(short = 'v', long, action = clap::ArgAction::Count)]
    verbose: u8,

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
        #[arg(short, long, value_enum, default_value_t = PolicyArg::Permissive)]
        policy: PolicyArg,
        /// Auto-continue without user prompts.
        #[arg(long)]
        auto_continue: bool,
    },
}

fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Commands::Run {
            task,
            policy,
            auto_continue,
        } => {
            let config = SupervisorConfig {
                policy: policy.into(),
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

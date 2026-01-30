//! Claude Supervisor - Automated Claude Code with AI oversight.

use std::io::{self, BufRead, Write};

use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use claude_supervisor::config::{ConfigLoader, PolicyConfig, SupervisorConfig};
use claude_supervisor::hooks::HookHandler;
use claude_supervisor::supervisor::{PolicyEngine, PolicyLevel};

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
        /// The task to execute (optional if --resume is used).
        task: Option<String>,
        /// Policy level (permissive, moderate, strict).
        #[arg(short, long, value_enum, default_value_t = PolicyArg::Permissive)]
        policy: PolicyArg,
        /// Auto-continue without user prompts.
        #[arg(long)]
        auto_continue: bool,
        /// Tools to auto-approve (comma-separated).
        #[arg(long, value_delimiter = ',')]
        allowed_tools: Option<Vec<String>>,
        /// Resume a previous session by ID.
        #[arg(long, conflicts_with = "task")]
        resume: Option<String>,
    },
    /// Install hooks into Claude Code settings.
    InstallHooks,
    /// Handle Claude Code hook events (reads JSON from stdin).
    Hook {
        #[command(subcommand)]
        event: HookEvent,
    },
    /// Configuration management.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
}

#[derive(Subcommand)]
enum HookEvent {
    /// Handle `PreToolUse` hook event.
    PreToolUse,
    /// Handle Stop hook event.
    Stop,
}

#[derive(Subcommand, Clone, Copy)]
enum ConfigAction {
    /// Show current configuration.
    Show,
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
        .with(fmt::layer().with_writer(io::stderr))
        .with(filter)
        .init();
}

fn build_policy_engine(config: &PolicyConfig) -> PolicyEngine {
    let mut engine = PolicyEngine::new(config.level);

    for tool in &config.tools.allowed {
        engine.allow_tool(tool);
    }

    for tool in &config.tools.denied {
        engine.deny_tool(tool);
    }

    engine
}

fn handle_hook(_event: HookEvent) {
    // Load configuration
    let loader = ConfigLoader::new();
    let config = match loader.load() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load config: {e}");
            std::process::exit(1);
        }
    };

    // Build policy engine from config
    let policy = build_policy_engine(&config);
    let handler = HookHandler::new(policy);

    // Read JSON from stdin
    let stdin = io::stdin();
    let mut input = String::new();
    for line in stdin.lock().lines() {
        match line {
            Ok(l) => input.push_str(&l),
            Err(e) => {
                eprintln!("Failed to read stdin: {e}");
                std::process::exit(1);
            }
        }
    }

    // Handle the hook event
    match handler.handle_json(&input) {
        Ok(result) => {
            // Write response to stdout
            if let Err(e) = io::stdout().write_all(result.response.as_bytes()) {
                eprintln!("Failed to write response: {e}");
                std::process::exit(1);
            }
            println!();

            // Exit with code 2 if deny
            if result.should_deny {
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("Hook error: {e}");
            std::process::exit(1);
        }
    }
}

fn handle_config(action: ConfigAction) {
    match action {
        ConfigAction::Show => {
            let loader = ConfigLoader::new();

            // Show where we're looking for config
            println!("# Config search paths:");
            for path in loader.search_paths() {
                let exists = if path.exists() { " (found)" } else { "" };
                println!("#   {}{exists}", path.display());
            }
            println!();

            // Load and display config
            match loader.load() {
                Ok(config) => {
                    println!("# Current configuration:");
                    match toml::to_string_pretty(&config) {
                        Ok(toml_str) => println!("{toml_str}"),
                        Err(e) => {
                            eprintln!("Failed to serialize config: {e}");
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to load config: {e}");
                    std::process::exit(1);
                }
            }
        }
    }
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
            allowed_tools,
            resume,
        } => {
            // Validate: either task or resume must be provided
            if task.is_none() && resume.is_none() {
                eprintln!("error: either <TASK> or --resume <SESSION_ID> is required");
                std::process::exit(1);
            }

            let mut config = SupervisorConfig {
                policy: policy.into(),
                auto_continue,
                ..Default::default()
            };

            // Wire allowed_tools to config
            if let Some(tools) = allowed_tools {
                config.allowed_tools = tools.into_iter().collect();
            }

            // Log based on task or resume mode
            if let Some(ref task_str) = task {
                tracing::info!(
                    task = %task_str,
                    policy = ?config.policy,
                    auto_continue = config.auto_continue,
                    allowed_tools = ?config.allowed_tools,
                    "Starting Claude supervisor"
                );
            } else if let Some(ref session_id) = resume {
                tracing::info!(
                    session_id = %session_id,
                    policy = ?config.policy,
                    auto_continue = config.auto_continue,
                    allowed_tools = ?config.allowed_tools,
                    "Resuming Claude supervisor session"
                );
            }

            tracing::warn!("Supervisor not yet implemented");
        }
        Commands::InstallHooks => {
            tracing::warn!("install-hooks not yet implemented (Phase 2)");
            eprintln!("install-hooks will be implemented in Phase 2");
        }
        Commands::Hook { event } => {
            handle_hook(event);
        }
        Commands::Config { action } => {
            handle_config(action);
        }
    }
}

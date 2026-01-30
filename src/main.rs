//! Claude Supervisor - Automated Claude Code with AI oversight.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

use claude_supervisor::ai::AiClient;
use claude_supervisor::cli::{ClaudeProcess, ClaudeProcessBuilder, SpawnError};
use claude_supervisor::commands::HookInstaller;
use claude_supervisor::config::{ConfigLoader, PolicyConfig, SupervisorConfig, WorktreeConfig};
use claude_supervisor::hooks::HookHandler;
use claude_supervisor::supervisor::{
    MultiSessionSupervisor, PolicyEngine, PolicyLevel, Supervisor, SupervisorResult,
};
use claude_supervisor::worktree::{WorktreeManager, WorktreeRegistry};

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
        /// Run in an isolated git worktree.
        #[arg(long)]
        worktree: bool,
        /// Custom worktree directory (default: .worktrees).
        #[arg(long)]
        worktree_dir: Option<PathBuf>,
        /// Cleanup worktree after session ends.
        #[arg(long)]
        worktree_cleanup: bool,
    },
    /// Install hooks into Claude Code settings.
    InstallHooks,
    /// Uninstall hooks from Claude Code settings.
    UninstallHooks,
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
    /// Manage git worktrees for session isolation.
    Worktree {
        #[command(subcommand)]
        action: WorktreeAction,
    },
    /// Run multiple Claude Code sessions in parallel.
    Multi {
        /// Tasks to run (can specify multiple).
        #[arg(long, action = clap::ArgAction::Append, required = true)]
        task: Vec<String>,
        /// Maximum parallel sessions.
        #[arg(long, default_value = "3")]
        max_parallel: usize,
        /// Policy level for all sessions.
        #[arg(short, long, value_enum, default_value_t = PolicyArg::Permissive)]
        policy: PolicyArg,
        /// Auto-continue without user prompts.
        #[arg(long)]
        auto_continue: bool,
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

#[derive(Subcommand, Clone)]
enum WorktreeAction {
    /// List all managed worktrees.
    List,
    /// Remove a worktree.
    Remove {
        /// Name of the worktree to remove.
        name: String,
        /// Force removal even with uncommitted changes.
        #[arg(short, long)]
        force: bool,
        /// Also delete the associated branch.
        #[arg(long)]
        delete_branch: bool,
    },
    /// Prune stale worktrees older than specified hours.
    Prune {
        /// Maximum age in hours (default: 24).
        #[arg(long, default_value = "24")]
        hours: u64,
        /// Force removal even with uncommitted changes.
        #[arg(short, long)]
        force: bool,
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

fn handle_install_hooks() {
    let installer = match HookInstaller::from_current_exe() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("Failed to create hook installer: {e}");
            std::process::exit(1);
        }
    };

    match installer.install() {
        Ok(result) => {
            println!("Hooks installed successfully!");
            println!("  Settings file: {}", result.settings_path.display());
            println!(
                "  PreToolUse: {}",
                if result.pre_tool_use_installed {
                    "installed"
                } else {
                    "skipped"
                }
            );
            println!(
                "  Stop: {}",
                if result.stop_installed {
                    "installed"
                } else {
                    "skipped"
                }
            );
            if result.replaced_existing {
                println!("  (Replaced existing supervisor hooks)");
            }
        }
        Err(e) => {
            eprintln!("Failed to install hooks: {e}");
            std::process::exit(1);
        }
    }
}

fn handle_uninstall_hooks() {
    let installer = match HookInstaller::from_current_exe() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("Failed to create hook installer: {e}");
            std::process::exit(1);
        }
    };

    match installer.uninstall() {
        Ok(result) => {
            println!("Hooks uninstalled successfully!");
            println!("  Settings file: {}", result.settings_path.display());
            println!(
                "  PreToolUse: {}",
                if result.pre_tool_use_removed {
                    "removed"
                } else {
                    "not found"
                }
            );
            println!(
                "  Stop: {}",
                if result.stop_removed {
                    "removed"
                } else {
                    "not found"
                }
            );
        }
        Err(e) => {
            eprintln!("Failed to uninstall hooks: {e}");
            std::process::exit(1);
        }
    }
}

#[allow(clippy::too_many_lines)]
async fn handle_worktree(action: WorktreeAction) {
    // Get current directory as repo root
    let repo_root = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            eprintln!("Failed to get current directory: {e}");
            std::process::exit(1);
        }
    };

    let config = WorktreeConfig::default();
    let manager = match WorktreeManager::new(repo_root, config) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("Failed to initialize worktree manager: {e}");
            std::process::exit(1);
        }
    };

    match action {
        WorktreeAction::List => {
            let registry_path = WorktreeRegistry::default_path(&manager.worktree_dir());
            let registry = match WorktreeRegistry::load(&registry_path) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Failed to load registry: {e}");
                    std::process::exit(1);
                }
            };

            let worktrees = registry.list();
            if worktrees.is_empty() {
                println!("No managed worktrees found.");
            } else {
                println!("Managed worktrees:");
                for wt in worktrees {
                    let status = match wt.status {
                        claude_supervisor::worktree::WorktreeStatus::Active => "active",
                        claude_supervisor::worktree::WorktreeStatus::Idle => "idle",
                        claude_supervisor::worktree::WorktreeStatus::PendingCleanup => "cleanup",
                    };
                    println!(
                        "  {} [{}] - {} ({})",
                        wt.name,
                        status,
                        wt.branch,
                        wt.path.display()
                    );
                }
            }
        }
        WorktreeAction::Remove {
            name,
            force,
            delete_branch,
        } => {
            // Load registry
            let registry_path = WorktreeRegistry::default_path(&manager.worktree_dir());
            let mut registry = match WorktreeRegistry::load(&registry_path) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Failed to load registry: {e}");
                    std::process::exit(1);
                }
            };

            // Get branch name before removal
            let branch_name = registry.get(&name).map(|wt| wt.branch.clone());

            // Remove worktree
            match manager.remove(&name, force).await {
                Ok(()) => {
                    println!("Worktree '{name}' removed.");

                    // Update registry
                    registry.remove(&name);
                    if let Err(e) = registry.save(&registry_path) {
                        eprintln!("Warning: Failed to update registry: {e}");
                    }

                    // Delete branch if requested
                    if delete_branch {
                        if let Some(branch) = branch_name {
                            match manager.delete_branch(&branch, force).await {
                                Ok(()) => println!("Branch '{branch}' deleted."),
                                Err(e) => eprintln!("Warning: Failed to delete branch: {e}"),
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to remove worktree: {e}");
                    std::process::exit(1);
                }
            }
        }
        WorktreeAction::Prune { hours, force } => {
            let registry_path = WorktreeRegistry::default_path(&manager.worktree_dir());
            let mut registry = match WorktreeRegistry::load(&registry_path) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("Failed to load registry: {e}");
                    std::process::exit(1);
                }
            };

            #[allow(clippy::cast_possible_wrap)]
            let max_age = chrono::Duration::hours(hours as i64);
            let stale: Vec<_> = registry
                .find_stale(max_age)
                .iter()
                .map(|wt| wt.name.clone())
                .collect();

            if stale.is_empty() {
                println!("No stale worktrees found (older than {hours} hours).");
                return;
            }

            println!("Pruning {} stale worktree(s)...", stale.len());
            for name in stale {
                match manager.remove(&name, force).await {
                    Ok(()) => {
                        registry.remove(&name);
                        println!("  Removed: {name}");
                    }
                    Err(e) => {
                        eprintln!("  Failed to remove '{name}': {e}");
                    }
                }
            }

            if let Err(e) = registry.save(&registry_path) {
                eprintln!("Warning: Failed to update registry: {e}");
            }
        }
    }
}

async fn handle_multi(
    tasks: Vec<String>,
    max_parallel: usize,
    policy: PolicyArg,
    _auto_continue: bool,
) {
    tracing::info!(
        tasks = tasks.len(),
        max_parallel = max_parallel,
        policy = ?policy,
        "Starting multi-session supervisor"
    );

    let policy_engine = PolicyEngine::new(policy.into());
    let mut supervisor = MultiSessionSupervisor::new(max_parallel, policy_engine);

    // Spawn all sessions
    for task in &tasks {
        match supervisor.spawn_session(task.clone()).await {
            Ok(id) => {
                tracing::info!(session_id = %id, task = %task, "Session spawned");
            }
            Err(e) => {
                tracing::error!(task = %task, error = %e, "Failed to spawn session");
            }
        }
    }

    // Wait for all to complete
    let results = supervisor.wait_all().await;

    // Print summary
    println!("\n=== Multi-Session Summary ===");
    println!("Sessions: {}", results.len());

    for result in &results {
        let status = match &result.result {
            Ok(r) => format!("{r:?}"),
            Err(e) => format!("Error: {e}"),
        };
        println!("  [{}] {} - {}", result.id, result.task, status);
    }

    let stats = supervisor.stats();
    println!("\n=== Aggregated Stats ===");
    println!("  Completed: {}", stats.sessions_completed);
    println!("  Failed: {}", stats.sessions_failed);
    println!("  Tool calls: {}", stats.total_tool_calls);
    println!("  Approvals: {}", stats.total_approvals);
    println!("  Denials: {}", stats.total_denials);
}

/// Handle the run command - spawn and supervise Claude Code.
async fn handle_run(
    task: Option<String>,
    resume: Option<String>,
    config: SupervisorConfig,
) -> Result<(), Box<dyn std::error::Error>> {
    // Handle worktree isolation if enabled
    let (working_dir, worktree_cleanup_info) = if config.worktree.enabled {
        tracing::info!("Creating isolated worktree for task");
        let repo_root = std::env::current_dir()?;
        let manager = WorktreeManager::new(repo_root, config.worktree.clone())?;
        let task_name = task.as_deref().unwrap_or("supervised-task").to_string();
        let worktree = manager.create(&task_name).await?;
        let path = worktree.path.clone();
        tracing::info!(path = %path.display(), "Running in worktree");
        (Some(path), Some((manager, task_name)))
    } else {
        (None, None)
    };

    // Get prompt (task or "continue" for resume)
    let prompt = task.unwrap_or_else(|| "continue".to_string());

    // Build process
    let mut builder = ClaudeProcessBuilder::new(&prompt);

    // Add resume if provided
    if let Some(ref session_id) = resume {
        builder = builder.resume(session_id);
    }

    // Add allowed tools if configured
    if !config.allowed_tools.is_empty() {
        let tools: Vec<&str> = config.allowed_tools.iter().map(String::as_str).collect();
        builder = builder.allowed_tools(&tools);
    }

    // Set working directory if using worktree
    if let Some(ref dir) = working_dir {
        builder = builder.working_dir(dir);
    }

    tracing::info!("Spawning Claude Code process");
    let process = ClaudeProcess::spawn(&builder)?;

    // Build policy engine
    let mut policy = PolicyEngine::new(config.policy);
    for tool in &config.allowed_tools {
        policy.allow_tool(tool);
    }

    // Create supervisor (with or without AI)
    let mut supervisor = if config.ai_supervisor {
        tracing::info!("AI supervision enabled");
        let ai_client = AiClient::from_env()?;
        Supervisor::from_process_with_ai(process, policy, ai_client)?
    } else {
        Supervisor::from_process(process, policy)?
    };

    // Set task context
    supervisor.set_task(&prompt);

    // Initialize knowledge from working directory (worktree or current)
    let cwd = std::env::current_dir()?;
    let knowledge_dir = working_dir.clone().unwrap_or_else(|| cwd.clone());
    supervisor.init_knowledge(&knowledge_dir).await;

    // Run supervision loop
    tracing::info!("Starting supervision loop");
    let result = supervisor.run().await?;

    // Report result
    match result {
        SupervisorResult::Completed {
            session_id,
            cost_usd,
        } => {
            tracing::info!(
                session_id = ?session_id,
                cost_usd = ?cost_usd,
                "Session completed successfully"
            );
        }
        SupervisorResult::Killed { reason } => {
            tracing::warn!(reason = %reason, "Session killed by supervisor");
        }
        SupervisorResult::ProcessExited => {
            tracing::info!("Claude process exited");
        }
        SupervisorResult::Cancelled => {
            tracing::info!("Session cancelled");
        }
    }

    // Cleanup worktree if configured
    if let Some((manager, task_name)) = worktree_cleanup_info {
        if config.worktree.auto_cleanup {
            tracing::info!(worktree = %task_name, "Cleaning up worktree");
            if let Err(e) = manager.remove(&task_name, false).await {
                tracing::warn!(error = %e, "Failed to cleanup worktree");
            }
        }
    }

    Ok(())
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
            worktree,
            worktree_dir,
            worktree_cleanup,
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

            // Configure worktree settings
            if worktree {
                config.worktree.enabled = true;
            }
            if let Some(dir) = worktree_dir {
                config.worktree.worktree_dir = dir;
            }
            if worktree_cleanup {
                config.worktree.auto_cleanup = true;
            }

            // Log based on task or resume mode
            if let Some(ref task_str) = task {
                tracing::info!(
                    task = %task_str,
                    policy = ?config.policy,
                    auto_continue = config.auto_continue,
                    allowed_tools = ?config.allowed_tools,
                    worktree_enabled = config.worktree.enabled,
                    "Starting Claude supervisor"
                );
            } else if let Some(ref session_id) = resume {
                tracing::info!(
                    session_id = %session_id,
                    policy = ?config.policy,
                    auto_continue = config.auto_continue,
                    allowed_tools = ?config.allowed_tools,
                    worktree_enabled = config.worktree.enabled,
                    "Resuming Claude supervisor session"
                );
            }

            if let Err(e) = handle_run(task, resume, config).await {
                // Provide user-friendly error messages for common failures
                if let Some(spawn_err) = e.downcast_ref::<SpawnError>() {
                    match spawn_err {
                        SpawnError::NotFound => {
                            eprintln!(
                                "error: Claude CLI not found. Is 'claude' installed and in PATH?"
                            );
                        }
                        SpawnError::PermissionDenied => {
                            eprintln!("error: Permission denied when spawning Claude CLI");
                        }
                        SpawnError::Io(_) => {
                            eprintln!("error: {e}");
                        }
                    }
                } else {
                    eprintln!("error: {e}");
                }
                tracing::error!(error = %e, "Supervisor failed");
                std::process::exit(1);
            }
        }
        Commands::InstallHooks => {
            handle_install_hooks();
        }
        Commands::UninstallHooks => {
            handle_uninstall_hooks();
        }
        Commands::Hook { event } => {
            handle_hook(event);
        }
        Commands::Config { action } => {
            handle_config(action);
        }
        Commands::Worktree { action } => {
            handle_worktree(action).await;
        }
        Commands::Multi {
            task,
            max_parallel,
            policy,
            auto_continue,
        } => {
            handle_multi(task, max_parallel, policy, auto_continue).await;
        }
    }
}

# CLI Interface Implementation Plan

> **REQUIRED:** Use `execute-plan` to implement this plan batch by batch.

**Goal:** Complete the CLI interface for claude-supervisor per Issue #8 requirements.

**Architecture:** Extend existing clap-based CLI with missing flags (`--allowed-tools`, `--resume`, `-v`) and add `InstallHooks` subcommand stub. Wire new arguments to `SupervisorConfig`.

**Tech Stack:** Rust, clap 4 (derive), tracing-subscriber

**Issue:** #8 (Phase 1: CLI interface)

**Worktree:** `.worktrees/issue-8-cli-interface`

---

### Batch 1: Verbosity Flags

**Goal:** Add `-v` verbosity flags that control tracing log level.

#### Task 1.1: Add verbosity flag to CLI struct

**Files:**
- Modify: `src/main.rs:32-35`

**Step 1: Write failing test**

No unit test needed - this is a clap derive change. Verify via CLI.

**Step 2: Implement**

Add `verbose` field to `Cli` struct before the subcommand:

```rust
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
```

**Step 3: Verify**

Run: `cargo run -- --help`

Expected output contains:
```
  -v, --verbose...  Increase verbosity (-v, -vv, -vvv)
```

**Step 4: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): add -v verbosity flag"
```

---

#### Task 1.2: Wire verbosity to tracing

**Files:**
- Modify: `src/main.rs:52-58` (init_tracing function)
- Modify: `src/main.rs:62` (call site)

**Step 1: Implement**

Update `init_tracing` to accept verbosity parameter:

```rust
fn init_tracing(verbosity: u8) {
    let level = match verbosity {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(level));
    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(filter)
        .init();
}
```

Update call site in `main()`:

```rust
init_tracing(cli.verbose);
```

**Step 2: Verify**

Run: `cargo run -- run "test task"`

Expected: No output (warn level, no warnings emitted yet)

Run: `cargo run -- -v run "test task"`

Expected: Shows info-level log "Starting Claude supervisor"

Run: `cargo run -- -vv run "test task"`

Expected: Shows debug-level logs

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): wire verbosity flag to tracing levels"
```

---

### Batch 2: Run Subcommand Flags

**Goal:** Add `--allowed-tools` and `--resume` flags to the Run subcommand.

#### Task 2.1: Add --allowed-tools flag

**Files:**
- Modify: `src/main.rs:40-49` (Commands::Run variant)
- Modify: `src/main.rs:71-75` (SupervisorConfig construction)

**Step 1: Implement**

Add `allowed_tools` field to Run variant:

```rust
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
        /// Tools to auto-approve (comma-separated).
        #[arg(long, value_delimiter = ',')]
        allowed_tools: Option<Vec<String>>,
    },
}
```

Update match arm to destructure and wire to config:

```rust
Commands::Run {
    task,
    policy,
    auto_continue,
    allowed_tools,
} => {
    let mut config = SupervisorConfig {
        policy: policy.into(),
        auto_continue,
        ..Default::default()
    };
    if let Some(tools) = allowed_tools {
        config.allowed_tools = tools.into_iter().collect();
    }
    tracing::info!(
        task = %task,
        policy = ?config.policy,
        auto_continue = config.auto_continue,
        allowed_tools = ?config.allowed_tools,
        "Starting Claude supervisor"
    );
    tracing::warn!("Supervisor not yet implemented");
}
```

**Step 2: Verify**

Run: `cargo run -- run --help`

Expected output contains:
```
      --allowed-tools <ALLOWED_TOOLS>  Tools to auto-approve (comma-separated)
```

Run: `cargo run -- -v run "test" --allowed-tools Read,Edit,Bash`

Expected: Log shows `allowed_tools = {"Read", "Edit", "Bash"}` (order may vary)

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): add --allowed-tools flag"
```

---

#### Task 2.2: Add --resume flag

**Files:**
- Modify: `src/main.rs:40-49` (Commands::Run variant)

**Step 1: Implement**

Add `resume` field to Run variant. Make `task` optional when resuming:

```rust
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
}
```

Update match arm to handle both cases:

```rust
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
    if let Some(tools) = allowed_tools {
        config.allowed_tools = tools.into_iter().collect();
    }

    if let Some(session_id) = &resume {
        tracing::info!(
            session_id = %session_id,
            policy = ?config.policy,
            "Resuming Claude session"
        );
    } else if let Some(task) = &task {
        tracing::info!(
            task = %task,
            policy = ?config.policy,
            auto_continue = config.auto_continue,
            allowed_tools = ?config.allowed_tools,
            "Starting Claude supervisor"
        );
    }
    tracing::warn!("Supervisor not yet implemented");
}
```

**Step 2: Verify**

Run: `cargo run -- run --help`

Expected output contains:
```
      --resume <RESUME>  Resume a previous session by ID
```

Run: `cargo run -- -v run --resume abc123`

Expected: Log shows "Resuming Claude session" with session_id=abc123

Run: `cargo run -- run`

Expected: Error "either <TASK> or --resume <SESSION_ID> is required"

Run: `cargo run -- run "task" --resume abc123`

Expected: Error about conflicting arguments

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): add --resume flag for session continuation"
```

---

### Batch 3: InstallHooks Subcommand

**Goal:** Add `InstallHooks` subcommand stub for Phase 2.

#### Task 3.1: Add InstallHooks subcommand

**Files:**
- Modify: `src/main.rs:37-50` (Commands enum)
- Modify: `src/main.rs:65-84` (match in main)

**Step 1: Implement**

Add new variant to Commands enum:

```rust
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
}
```

Add match arm for InstallHooks:

```rust
Commands::InstallHooks => {
    tracing::warn!("install-hooks not yet implemented (Phase 2)");
    eprintln!("install-hooks will be implemented in Phase 2");
}
```

**Step 2: Verify**

Run: `cargo run -- --help`

Expected output contains:
```
Commands:
  run            Run Claude Code with supervision
  install-hooks  Install hooks into Claude Code settings
```

Run: `cargo run -- install-hooks`

Expected: Message "install-hooks will be implemented in Phase 2"

**Step 3: Commit**

```bash
git add src/main.rs
git commit -m "feat(cli): add install-hooks subcommand stub"
```

---

### Batch 4: Final Verification

**Goal:** Verify all CLI features work together and update help text.

#### Task 4.1: Integration verification

**Files:**
- None (verification only)

**Step 1: Verify all commands**

Run full help:
```bash
cargo run -- --help
```

Expected:
```
Automated Claude Code with AI oversight

Usage: claude-supervisor [OPTIONS] <COMMAND>

Commands:
  run            Run Claude Code with supervision
  install-hooks  Install hooks into Claude Code settings
  help           Print this message or the help of the given subcommand(s)

Options:
  -v, --verbose...  Increase verbosity (-v, -vv, -vvv)
  -h, --help        Print help
  -V, --version     Print version
```

Run subcommand help:
```bash
cargo run -- run --help
```

Expected:
```
Run Claude Code with supervision

Usage: claude-supervisor run [OPTIONS] [TASK]

Arguments:
  [TASK]  The task to execute (optional if --resume is used)

Options:
  -p, --policy <POLICY>              Policy level (permissive, moderate, strict) [default: permissive]
      --auto-continue                Auto-continue without user prompts
      --allowed-tools <ALLOWED_TOOLS>  Tools to auto-approve (comma-separated)
      --resume <RESUME>              Resume a previous session by ID
  -h, --help                         Print help
```

**Step 2: Run clippy and tests**

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo t
```

Expected: No warnings, all tests pass

**Step 3: Final commit**

```bash
git add -A
git commit -m "docs: update CLI help text" --allow-empty
```

---

## Acceptance Criteria Checklist

| Requirement | Task | Status |
|-------------|------|--------|
| `run` subcommand with task argument | Already done | ✓ |
| `--policy` flag with strict/moderate/permissive | Already done | ✓ |
| `--allowed-tools` flag | Task 2.1 | |
| `--resume` flag | Task 2.2 | |
| `-v` verbosity flags | Task 1.1, 1.2 | |
| `install-hooks` subcommand (stub) | Task 3.1 | |
| Help text for all commands | Task 4.1 | |

## Post-Implementation

After all batches complete:
1. Run `cargo t` to verify no regressions
2. Create PR to merge `issue-8-cli-interface` into `main`
3. Close Issue #8

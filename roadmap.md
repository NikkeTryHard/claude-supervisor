# Claude Supervisor Roadmap

Automated Claude Code with AI oversight — a Rust-based supervisor that monitors Claude Code execution, prevents hallucinations, and can intervene in real-time.

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│                    RUST SUPERVISOR                          │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐  │
│  │ Policy Engine│◄───│  AI Client   │◄───│    clust     │  │
│  │ (rules-based)│    │ (supervisor) │    │ (Claude API) │  │
│  └──────┬───────┘    └──────────────┘    └──────────────┘  │
│         │ approve/deny/guide                                │
│         ▼                                                   │
│  ┌──────────────────────────────────────┐                  │
│  │      Event Router (mpsc channel)     │                  │
│  └──────────────────────────────────────┘                  │
│         ▲                        ▲                         │
└─────────┼────────────────────────┼─────────────────────────┘
          │                        │
    ┌─────┴─────┐           ┌──────┴──────┐
    │ CLI Stream│           │ Hook Handler│
    │ (stdout)  │           │ (PreToolUse)│
    └─────┬─────┘           └──────┬──────┘
          │                        │
    ┌─────┴─────────────────┐      │
    │ claude -p "<task>"    │◄─────┘
    │ --output-format       │  (blocks/allows)
    │ stream-json           │
    └───────────────────────┘
```

## Key Findings from Research

### Claude Code Native APIs (No PTY Needed)

| Feature | Command/Flag |
|---------|--------------|
| Non-interactive mode | `claude -p "prompt"` |
| Structured output | `--output-format stream-json` |
| Auto-approve tools | `--allowedTools "Read,Edit,Bash"` |
| Session resume | `--resume <session_id>` |
| JSON schema output | `--json-schema '{...}'` |

### Hooks System (12 Events)

| Hook | Use Case |
|------|----------|
| `PreToolUse` | Block/allow tool calls before execution |
| `PostToolUse` | Feedback after tool succeeds |
| `Stop` | Prevent session from ending (force continue) |
| `UserPromptSubmit` | Inject context before Claude receives input |
| `SessionStart` | Set environment variables, inject context |

### Conversation Storage

- **Location**: `~/.claude/projects/<encoded-path>/<session-uuid>.jsonl`
- **Format**: Append-only JSONL with `type`, `uuid`, `parentUuid`, `message`
- **Subagents**: `<session>/subagents/agent-<id>.jsonl`

---

## Phase 1: Minimal Viable Supervisor

**Goal**: Stream parser + basic policy engine

### Tasks

- [ ] Initialize Rust project with Cargo
- [ ] Add core dependencies (tokio, serde_json, thiserror, tracing)
- [ ] Implement CLI spawner (`claude -p` with stream-json)
- [ ] Implement stream parser (line-by-line JSONL)
- [ ] Define event types (ToolUse, ContentBlock, Stop, etc.)
- [ ] Basic policy engine (blocklist patterns, file path rules)
- [ ] Process control (kill on policy violation)
- [ ] CLI interface for supervisor (`claude-supervisor run "<task>"`)

### Crates

```toml
[dependencies]
tokio = { version = "1", features = ["full", "process"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "1"
tracing = "0.1"
tracing-subscriber = "0.3"
clap = { version = "4", features = ["derive"] }
```

### Deliverable

```bash
claude-supervisor run "Fix the bug in auth.rs" --policy strict
```

---

## Phase 2: Hook Integration

**Goal**: Real-time intervention via Claude Code hooks

### Tasks

- [ ] Create hook handler binary (`claude-supervisor-hook`)
- [ ] Implement PreToolUse handler (approve/deny/escalate)
- [ ] Implement Stop handler (force continue if incomplete)
- [ ] IPC between supervisor and hook (Unix socket or file-based)
- [ ] Hook installer (`claude-supervisor install-hooks`)
- [ ] Policy configuration file (TOML/JSON)

### Hook Configuration

```json
{
  "hooks": {
    "PreToolUse": [{
      "type": "command",
      "command": "claude-supervisor-hook pre-tool-use",
      "timeout": 5
    }],
    "Stop": [{
      "type": "command",
      "command": "claude-supervisor-hook stop",
      "timeout": 3
    }]
  }
}
```

### Deliverable

Hooks that can block dangerous operations and force continuation.

---

## Phase 3: AI Supervisor

**Goal**: LLM-based decision making for ambiguous cases

### Tasks

- [ ] Add clust crate for Claude API calls
- [ ] Define supervisor system prompt (rules, context awareness)
- [ ] Implement escalation flow (policy uncertain → ask AI)
- [ ] Context compression for supervisor (summarize conversation)
- [ ] Decision caching (avoid repeated API calls for same patterns)
- [ ] Cost tracking (token usage per session)

### Crates

```toml
[dependencies]
clust = "0.9"
```

### Supervisor Prompt Structure

```
You are a code review supervisor monitoring an AI coding assistant.

CONTEXT:
- Project: {project_name}
- Rules: {claude_md_content}
- Current task: {task}

RECENT ACTIONS:
{last_n_events}

QUESTION:
Should the agent be allowed to: {pending_action}

Respond: ALLOW, DENY, or GUIDE with correction.
```

### Deliverable

AI-powered judgment for edge cases the policy engine can't handle.

---

## Phase 4: Conversation Awareness

**Goal**: Full context from JSONL files

### Tasks

- [ ] Add notify crate for file watching
- [ ] JSONL parser for conversation history
- [ ] Session state reconstruction from files
- [ ] Subagent tracking (monitor delegated tasks)
- [ ] Historical pattern detection (repeated failures, loops)

### Crates

```toml
[dependencies]
notify = "6"
```

### Deliverable

Supervisor has full conversation context, can detect patterns across turns.

---

## Phase 5: Advanced Features

**Goal**: Production-ready autonomous operation

### Tasks

- [ ] Auto-continue mode (restart on completion with follow-up)
- [ ] Git worktree isolation (safe experimentation)
- [ ] Audit logging (all decisions, all events)
- [ ] Web dashboard (optional, via axum)
- [ ] Multi-session management (parallel Claude instances)
- [ ] Memory layer (cross-session learning)

### Crates

```toml
[dependencies]
axum = "0.7"           # Optional: web dashboard
rusqlite = "0.31"      # Audit logging
git2 = "0.18"          # Worktree management
```

---

## Project Structure

```
claude-supervisor/
├── Cargo.toml
├── src/
│   ├── main.rs              # CLI entry point
│   ├── lib.rs               # Library exports
│   ├── cli/
│   │   ├── mod.rs           # CLI spawning
│   │   ├── stream.rs        # stream-json parser
│   │   └── events.rs        # Event type definitions
│   ├── hooks/
│   │   ├── mod.rs           # Hook handler binary logic
│   │   ├── pre_tool_use.rs  # PreToolUse decisions
│   │   └── stop.rs          # Stop event handling
│   ├── supervisor/
│   │   ├── mod.rs           # Orchestration
│   │   ├── policy.rs        # Rules engine
│   │   └── state.rs         # Session state machine
│   ├── ai/
│   │   ├── mod.rs           # AI client wrapper
│   │   ├── client.rs        # clust integration
│   │   └── prompts.rs       # Supervisor prompts
│   └── config/
│       ├── mod.rs           # Config loading
│       └── types.rs         # Config structs
├── hooks/
│   └── hooks.json           # Claude Code hook config
└── tests/
    └── integration/
```

---

## References

### Existing Projects

| Project | Approach | Notes |
|---------|----------|-------|
| [Auto Claude](https://github.com/ruizrica/auto-claude) | File-based orchestration | 12 parallel terminals, QA agents |
| [Continuous Claude](https://github.com/AnandChowdhary/continuous-claude) | Loop + SHARED_TASK_NOTES.md | PR creation, CI integration |
| [Claude Code Guardrails](https://github.com/wangbooth/Claude-Code-Guardrails) | Hooks | Branch protection, checkpointing |

### Rust Crates

| Crate | Purpose |
|-------|---------|
| `clust` | Anthropic API client (streaming, tools) |
| `claude-sdk` | Alternative Claude client |
| `rig` | Agent framework (if needed) |
| `notify` | File system watching |
| `tokio` | Async runtime |

### Claude Code Documentation

- [Headless Mode](https://code.claude.com/docs/en/headless) — `-p` flag, output formats
- [Hooks](https://code.claude.com/docs/en/hooks) — Event types, configuration
- [Agent SDK](https://code.claude.com/docs/en/sdk) — Python/TypeScript alternatives

---

## Success Criteria

1. **Phase 1**: Can run Claude Code tasks with stream monitoring and kill on violation
2. **Phase 2**: Hooks intercept dangerous operations before execution
3. **Phase 3**: AI supervisor makes judgment calls on ambiguous cases
4. **Phase 4**: Full conversation context informs decisions
5. **Phase 5**: Fully autonomous operation with audit trail

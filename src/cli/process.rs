//! Claude Code process spawning and control.
//!
//! This module provides a builder pattern for configuring and spawning
//! Claude Code processes, along with control methods for managing the
//! running process.

use std::path::PathBuf;
use std::process::{ExitStatus, Stdio};
use std::time::Duration;

use tokio::process::{Child, ChildStderr, ChildStdout, Command};

/// Error type for process spawning operations.
#[derive(thiserror::Error, Debug)]
pub enum SpawnError {
    /// The binary was not found.
    #[error("Claude binary not found")]
    NotFound,
    /// Permission denied when spawning.
    #[error("Permission denied")]
    PermissionDenied,
    /// Other I/O error.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

impl SpawnError {
    /// Create a `SpawnError` from an I/O error, classifying common cases.
    fn from_io(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied,
            _ => Self::Io(err),
        }
    }
}

/// Builder for configuring Claude Code process arguments.
#[derive(Debug, Clone, Default)]
pub struct ClaudeProcessBuilder {
    prompt: String,
    allowed_tools: Option<Vec<String>>,
    resume_session: Option<String>,
    max_turns: Option<u32>,
    append_system_prompt: Option<String>,
    system_prompt: Option<String>,
    working_dir: Option<PathBuf>,
}

impl ClaudeProcessBuilder {
    /// Create a new builder with the given prompt.
    #[must_use]
    pub fn new(prompt: impl Into<String>) -> Self {
        Self {
            prompt: prompt.into(),
            ..Default::default()
        }
    }

    /// Set the allowed tools for this session.
    #[must_use]
    pub fn allowed_tools(mut self, tools: &[&str]) -> Self {
        self.allowed_tools = Some(tools.iter().map(|s| (*s).to_string()).collect());
        self
    }

    /// Resume an existing session.
    #[must_use]
    pub fn resume(mut self, session_id: impl Into<String>) -> Self {
        self.resume_session = Some(session_id.into());
        self
    }

    /// Set the maximum number of turns.
    #[must_use]
    pub fn max_turns(mut self, turns: u32) -> Self {
        self.max_turns = Some(turns);
        self
    }

    /// Append to the system prompt.
    #[must_use]
    pub fn append_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.append_system_prompt = Some(prompt.into());
        self
    }

    /// Set a custom system prompt.
    #[must_use]
    pub fn system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set the working directory for the Claude process.
    #[must_use]
    pub fn working_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_dir = Some(dir.into());
        self
    }

    /// Get the working directory, if set.
    #[must_use]
    pub fn get_working_dir(&self) -> Option<&PathBuf> {
        self.working_dir.as_ref()
    }

    /// Get the prompt.
    #[must_use]
    pub fn prompt(&self) -> &str {
        &self.prompt
    }

    /// Build the command-line arguments.
    #[must_use]
    pub fn build_args(&self) -> Vec<String> {
        let mut args = vec![
            "-p".to_string(),
            self.prompt.clone(),
            "--output-format".to_string(),
            "stream-json".to_string(),
        ];

        if let Some(tools) = &self.allowed_tools {
            args.push("--allowedTools".to_string());
            args.push(tools.join(","));
        }

        if let Some(session_id) = &self.resume_session {
            args.push("--resume".to_string());
            args.push(session_id.clone());
        }

        if let Some(turns) = self.max_turns {
            args.push("--max-turns".to_string());
            args.push(turns.to_string());
        }

        if let Some(prompt) = &self.append_system_prompt {
            args.push("--append-system-prompt".to_string());
            args.push(prompt.clone());
        }

        if let Some(prompt) = &self.system_prompt {
            args.push("--system-prompt".to_string());
            args.push(prompt.clone());
        }

        args
    }
}

/// A running Claude Code process.
#[derive(Debug)]
pub struct ClaudeProcess {
    child: Child,
}

impl ClaudeProcess {
    /// Spawn a Claude Code process with the given builder configuration.
    ///
    /// # Errors
    ///
    /// Returns `SpawnError` if the process fails to spawn.
    pub fn spawn(builder: &ClaudeProcessBuilder) -> Result<Self, SpawnError> {
        Self::spawn_with_binary("claude", builder)
    }

    /// Spawn a process using a custom binary (for testing).
    ///
    /// # Errors
    ///
    /// Returns `SpawnError` if the process fails to spawn.
    pub fn spawn_with_binary(
        binary: &str,
        builder: &ClaudeProcessBuilder,
    ) -> Result<Self, SpawnError> {
        let args = builder.build_args();

        let mut cmd = Command::new(binary);
        cmd.args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Apply working directory if set
        if let Some(ref dir) = builder.working_dir {
            cmd.current_dir(dir);
        }

        let child = cmd.spawn().map_err(SpawnError::from_io)?;

        Ok(Self { child })
    }

    /// Take ownership of the stdout handle.
    ///
    /// This can only be called once; subsequent calls return `None`.
    pub fn take_stdout(&mut self) -> Option<ChildStdout> {
        self.child.stdout.take()
    }

    /// Take ownership of the stderr handle.
    ///
    /// This can only be called once; subsequent calls return `None`.
    pub fn take_stderr(&mut self) -> Option<ChildStderr> {
        self.child.stderr.take()
    }

    /// Get the process ID, if still running.
    #[must_use]
    pub fn id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Check if the process has exited without blocking.
    ///
    /// # Errors
    ///
    /// Returns an error if the process state cannot be queried.
    pub fn try_wait(&mut self) -> std::io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }

    /// Wait for the process to exit.
    ///
    /// # Errors
    ///
    /// Returns an error if waiting fails.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        self.child.wait().await
    }

    /// Forcefully kill the process.
    ///
    /// # Errors
    ///
    /// Returns an error if the kill signal cannot be sent.
    pub async fn kill(&mut self) -> std::io::Result<()> {
        self.child.kill().await
    }

    /// Attempt graceful termination with a timeout.
    ///
    /// On Unix, sends SIGTERM first, then SIGKILL after the timeout.
    /// On other platforms, falls back to immediate kill.
    ///
    /// # Errors
    ///
    /// Returns an error if termination fails.
    pub async fn graceful_terminate(&mut self, timeout: Duration) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            self.graceful_terminate_unix(timeout).await
        }

        #[cfg(not(unix))]
        {
            let _ = timeout;
            self.kill().await
        }
    }

    #[cfg(unix)]
    async fn graceful_terminate_unix(&mut self, timeout: Duration) -> std::io::Result<()> {
        use nix::sys::signal::{kill, Signal};
        use nix::unistd::Pid;

        if let Some(pid) = self.id() {
            // Send SIGTERM
            let nix_pid = Pid::from_raw(i32::try_from(pid).unwrap_or(i32::MAX));
            let _ = kill(nix_pid, Signal::SIGTERM);

            // Wait with timeout
            let wait_result = tokio::time::timeout(timeout, self.child.wait()).await;

            match wait_result {
                Ok(Ok(_)) => Ok(()),
                Ok(Err(e)) => Err(e),
                Err(_) => {
                    // Timeout elapsed, force kill
                    self.child.kill().await
                }
            }
        } else {
            // Process already exited
            Ok(())
        }
    }
}

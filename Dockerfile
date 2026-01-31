# Claude Supervisor - Isolated Development Environment
#
# Build:   docker build -t claude-supervisor .
# Run:     docker run -it --rm -e GEMINI_API_KEY=$GEMINI_API_KEY claude-supervisor
# Shell:   docker run -it --rm -e GEMINI_API_KEY=$GEMINI_API_KEY claude-supervisor bash

FROM rust:1.85-bookworm AS builder

WORKDIR /app

# Install build dependencies
RUN apt-get update && apt-get install -y \
    mold \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Copy source
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build release binary
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    git \
    ripgrep \
    && rm -rf /var/lib/apt/lists/*

# Copy supervisor binary (must happen as root)
COPY --from=builder /app/target/release/claude-supervisor /usr/local/bin/

# Create non-root user
RUN useradd -m -s /bin/bash claude

# Create directories and copy config (as root, then chown)
RUN mkdir -p /home/claude/.claude \
    && mkdir -p /home/claude/.config/claude-supervisor \
    && chown -R claude:claude /home/claude/.claude /home/claude/.config
COPY --chown=claude:claude config.example.toml /home/claude/.config/claude-supervisor/config.toml

# Switch to claude user for Claude Code installation
USER claude
WORKDIR /home/claude

# Install Claude Code CLI (native installer - installs to ~/.claude/bin)
RUN curl -fsSL https://claude.ai/install.sh | bash

# Add Claude Code to PATH (native installer uses ~/.local/bin)
ENV PATH="/home/claude/.local/bin:${PATH}"

# Default environment
ENV CLAUDE_HOME=/home/claude/.claude
ENV RUST_LOG=info

# Expose dashboard port
EXPOSE 3000

# Default command
ENTRYPOINT ["claude-supervisor"]
CMD ["--help"]

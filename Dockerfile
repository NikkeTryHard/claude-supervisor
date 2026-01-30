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
    nodejs \
    npm \
    && rm -rf /var/lib/apt/lists/*

# Install Claude Code CLI
RUN npm install -g @anthropic-ai/claude-code

# Create non-root user
RUN useradd -m -s /bin/bash claude
USER claude
WORKDIR /home/claude

# Create isolated .claude directory
RUN mkdir -p /home/claude/.claude \
    && mkdir -p /home/claude/.config/claude-supervisor

# Copy binary from builder
COPY --from=builder /app/target/release/claude-supervisor /usr/local/bin/

# Copy example config
COPY --chown=claude:claude config.example.toml /home/claude/.config/claude-supervisor/config.toml

# Default environment
ENV CLAUDE_HOME=/home/claude/.claude
ENV RUST_LOG=info

# Expose dashboard port
EXPOSE 3000

# Default command
ENTRYPOINT ["claude-supervisor"]
CMD ["--help"]

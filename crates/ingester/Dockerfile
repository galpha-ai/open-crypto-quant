# Multi-stage build for Rust ingester service using cargo-chef
FROM lukemathwalker/cargo-chef:latest-rust-1.87-slim AS chef
WORKDIR /app

# Install required system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    build-essential \
    git \
    openssh-client \
    && rm -rf /var/lib/apt/lists/*

# Install nightly toolchain
RUN rustup toolchain install nightly && rustup default nightly

# Plan stage - prepare the recipe for dependency caching
FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# Build stage - build dependencies and then the application
FROM chef AS builder

# Copy the recipe and build dependencies (this layer will be cached)
COPY --from=planner /app/recipe.json recipe.json

# Build dependencies
RUN --mount=type=ssh \
    mkdir -p /root/.ssh && \
    ssh-keyscan github.com >> /root/.ssh/known_hosts && \
    cargo chef cook --release --recipe-path recipe.json

# Copy the source code and build the application
COPY . .
RUN --mount=type=ssh \
    cargo build --release -p ingester

# Runtime stage - use a minimal base image
FROM debian:bookworm-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user
RUN useradd --create-home --shell /bin/bash app

# Set working directory
WORKDIR /app

# Copy the built binary from builder stage
COPY --from=builder /app/target/release/ingester /usr/local/bin/ingester

# Make sure the binary is executable
RUN chmod +x /usr/local/bin/ingester

# Change ownership of the app directory
RUN chown -R app:app /app

# Switch to non-root user
USER app

# Set the entrypoint
ENTRYPOINT ["/usr/local/bin/ingester"]

# Default command (config file path should be provided at runtime)
CMD ["--config", "/app/config.yaml"]
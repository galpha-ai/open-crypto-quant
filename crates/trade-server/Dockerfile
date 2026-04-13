# Use slim Rust image for building and testing
FROM rust:1.87-slim

# Install required system dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    ca-certificates \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Install nightly toolchain
RUN rustup toolchain install nightly && rustup default nightly

# Set working directory
WORKDIR /app

# Copy entire workspace (trade_server depends on other crates)
COPY . .

# Run tests for trade_server package
CMD ["cargo", "test", "--package", "trade_server", "--verbose"]
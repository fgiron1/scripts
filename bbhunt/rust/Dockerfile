# Multi-stage build for Rust
FROM rust:1.75-slim-bullseye AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    curl \
    wget \
    git \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Install additional tools (optional, adjust as needed)
RUN mkdir -p /tools
WORKDIR /tools

# Install Go for additional tools
RUN wget https://golang.org/dl/go1.21.5.linux-amd64.tar.gz \
    && tar -C /usr/local -xzf go1.21.5.linux-amd64.tar.gz
ENV PATH=$PATH:/usr/local/go/bin

# Install Go-based security tools
RUN go install -v github.com/projectdiscovery/subfinder/v2/cmd/subfinder@latest \
    && go install -v github.com/projectdiscovery/nuclei/v2/cmd/nuclei@latest \
    && go install -v github.com/tomnomnom/assetfinder@latest

# Set up Rust project
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY src ./src

# Build dependencies first for caching
RUN cargo build --release

# Final stage
FROM debian:bullseye-slim

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Copy built binary and tools
COPY --from=builder /app/target/release/bbhunt /usr/local/bin/bbhunt
COPY --from=builder /tools/bin/* /usr/local/bin/

# Create necessary directories
RUN mkdir -p /data /config

# Set working directory
WORKDIR /data

# Default entrypoint
ENTRYPOINT ["bbhunt"]
CMD ["--help"]

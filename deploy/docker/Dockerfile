# Multi-stage Dockerfile for mcpkit MCP servers
# Optimized for production deployments with minimal image size
#
# Build: docker build -f deploy/docker/Dockerfile -t mcpkit-server .
# Run:   docker run -p 3000:3000 mcpkit-server

# ============================================================================
# Stage 1: Build environment
# ============================================================================
FROM rust:1.85-bookworm AS builder

# Install required build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy only what's needed for dependency resolution first (for better caching)
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY mcpkit ./mcpkit
COPY examples ./examples

# Build the http-server example in release mode
# Change the target if building a different example
RUN cargo build --release -p http-server-example

# ============================================================================
# Stage 2: Runtime environment
# ============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    curl \
    libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -r -s /bin/false mcpkit

WORKDIR /app

# Copy the compiled binary from the builder stage
COPY --from=builder /app/target/release/http-server-example /app/mcp-server

# Set proper permissions
RUN chown mcpkit:mcpkit /app/mcp-server && chmod +x /app/mcp-server

# Switch to non-root user for security
USER mcpkit

# Environment variables for configuration
ENV MCP_BIND_ADDR=0.0.0.0:3000
ENV RUST_LOG=info

# Expose the MCP server port
EXPOSE 3000

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

# Run the MCP server
CMD ["/app/mcp-server"]

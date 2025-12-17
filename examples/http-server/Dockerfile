# mcpkit HTTP Server Example - Dockerfile
#
# Multi-stage build for optimal image size and security.
# Final image runs as non-root user and includes only the binary.
#
# Build: docker build -t mcpkit-http-server .
# Run:   docker run -p 3000:3000 mcpkit-http-server

# ============================================================
# Stage 1: Build
# ============================================================
FROM rust:1.85-bookworm AS builder

WORKDIR /build

# Copy workspace files (needed for workspace dependencies)
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY mcpkit/ mcpkit/
COPY examples/ examples/

# Build the http-server example in release mode
RUN cargo build --release --package http-server-example

# ============================================================
# Stage 2: Runtime
# ============================================================
FROM debian:bookworm-slim AS runtime

# Install CA certificates and curl (for healthcheck), create non-root user
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/* && \
    useradd --create-home --shell /bin/bash mcpkit

# Copy the binary from builder
COPY --from=builder /build/target/release/http-server-example /usr/local/bin/mcpkit-http-server

# Set ownership and permissions
RUN chown mcpkit:mcpkit /usr/local/bin/mcpkit-http-server && \
    chmod +x /usr/local/bin/mcpkit-http-server

# Switch to non-root user
USER mcpkit
WORKDIR /home/mcpkit

# Expose the MCP HTTP port
EXPOSE 3000

# Configure environment
ENV RUST_LOG=info
ENV MCP_BIND_ADDR=0.0.0.0:3000

# Health check
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -sf http://localhost:3000/health || exit 1

# Run the server
CMD ["mcpkit-http-server"]

//! MCP client and server using gRPC transport.
//!
//! This example demonstrates how to use the gRPC transport for MCP communication.
//! The gRPC transport provides high-performance, bidirectional streaming over HTTP/2.
//!
//! ## Features
//!
//! - HTTP/2 transport with multiplexing
//! - TLS support for secure connections
//! - Configurable timeouts and keepalive
//! - Custom metadata support
//! - Full bidirectional streaming support
//!
//! ## Running the example
//!
//! ```bash
//! cargo run -p grpc-client-example
//! ```
//!
//! The example starts a gRPC server and connects a client to it, demonstrating
//! bidirectional message exchange.

use mcpkit_core::protocol::{Message, Request};
use mcpkit_transport::grpc::{GrpcConfig, GrpcServer, GrpcServerBuilder, GrpcServerConfig, GrpcTransport};
use mcpkit_transport::Transport;
use std::time::Duration;
use tracing::{error, info};

/// Demonstrates gRPC transport configuration options.
fn demonstrate_client_config() {
    info!("=== gRPC Client Configuration ===");

    // Basic configuration
    let basic_config = GrpcConfig::new("http://localhost:50051");
    info!("Basic config: endpoint={}", basic_config.endpoint);

    // Full configuration with all options
    let full_config = GrpcConfig::new("https://mcp.example.com:50051")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(60))
        .with_tls()
        .with_metadata("authorization", "Bearer token123")
        .with_metadata("x-request-id", "req-001");

    info!(
        "Full config: endpoint={}, tls={}, connect_timeout={:?}, timeout={:?}",
        full_config.endpoint, full_config.tls, full_config.connect_timeout, full_config.timeout
    );
    info!("Metadata entries: {}", full_config.metadata.len());
}

/// Demonstrates gRPC server configuration options.
///
/// Note: The gRPC server implementation is currently a work-in-progress.
/// This shows the intended API for when the server is fully implemented.
fn demonstrate_server_config() {
    info!("=== gRPC Server Configuration ===");

    // Basic server configuration
    let basic_config = GrpcServerConfig::new("0.0.0.0:50051");
    info!("Basic server config: addr={}", basic_config.addr);

    // Full server configuration
    let full_config = GrpcServerConfig::new("0.0.0.0:50051")
        .with_tls()
        .max_concurrent_streams(200)
        .tcp_keepalive(Duration::from_secs(60))
        .http2_keepalive_interval(Duration::from_secs(30));

    info!(
        "Full server config: addr={}, tls={}, max_streams={:?}",
        full_config.addr, full_config.tls, full_config.max_concurrent_streams
    );

    // Using the builder pattern
    let server = GrpcServerBuilder::new("0.0.0.0:50051")
        .with_tls()
        .max_concurrent_streams(100)
        .tcp_keepalive(Duration::from_secs(120))
        .http2_keepalive_interval(Duration::from_secs(60))
        .build();

    info!(
        "Server from builder: addr={}, running={}",
        server.addr(),
        server.is_running()
    );
}

/// Demonstrates connecting to a gRPC server.
async fn demonstrate_client_connection() {
    info!("=== gRPC Client Connection ===");

    // Attempt to connect to a local gRPC server
    let config = GrpcConfig::new("http://localhost:50051")
        .connect_timeout(Duration::from_secs(2))
        .timeout(Duration::from_secs(10));

    info!("Attempting to connect to {}", config.endpoint);

    match GrpcTransport::connect(config).await {
        Ok(transport) => {
            info!("Connected successfully!");
            info!("Transport metadata: {:?}", transport.metadata());
            info!("Is connected: {}", transport.is_connected());

            // Demonstrate sending a ping message
            let ping = Message::Request(Request::new("ping", 1u64));

            info!("Sending ping message...");
            if let Err(e) = transport.send(ping).await {
                error!("Failed to send message: {e}");
            }

            // Close the transport
            if let Err(e) = transport.close().await {
                error!("Failed to close transport: {e}");
            }
            info!("Transport closed");
        }
        Err(e) => {
            info!(
                "Could not connect (expected if no server is running): {e}"
            );
        }
    }
}

/// Demonstrates the gRPC server API with a full client-server interaction.
async fn demonstrate_server_api() {
    info!("=== gRPC Server API ===");

    let config = GrpcServerConfig::new("127.0.0.1:50052");
    let server = std::sync::Arc::new(GrpcServer::new(config));

    info!("Server created: addr={}", server.addr());

    // Start the server
    if let Err(e) = server.clone().start().await {
        error!("Failed to start server: {e}");
        return;
    }
    info!("Server started and listening");

    // Give the server a moment to start
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Spawn a task to accept connections
    let server_handle = server.clone();
    let accept_task = tokio::spawn(async move {
        info!("Waiting for incoming connection...");
        if let Some(transport) = server_handle.accept().await {
            info!("Accepted connection from: {:?}", transport.metadata().remote_addr);

            // Echo back any received messages
            if let Ok(Some(msg)) = transport.recv().await {
                info!("Server received: {:?}", msg);

                // Create a response
                let response = mcpkit_core::protocol::Message::Response(
                    mcpkit_core::protocol::Response::success(
                        1u64,
                        serde_json::json!({"status": "pong"}),
                    )
                );

                if let Err(e) = transport.send(response).await {
                    error!("Server failed to send response: {e}");
                }
            }
        }
    });

    // Connect a client
    let client_config = GrpcConfig::new("http://127.0.0.1:50052")
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(10));

    info!("Client connecting to {}", client_config.endpoint);

    match GrpcTransport::connect(client_config).await {
        Ok(transport) => {
            info!("Client connected!");

            // Send a ping message
            let ping = Message::Request(Request::new("ping", 1u64));
            info!("Client sending ping...");

            if let Err(e) = transport.send(ping).await {
                error!("Client failed to send: {e}");
            }

            // Try to receive a response (with timeout)
            match tokio::time::timeout(Duration::from_secs(2), transport.recv()).await {
                Ok(Ok(Some(msg))) => {
                    info!("Client received response: {:?}", msg);
                }
                Ok(Ok(None)) => {
                    info!("Client: connection closed");
                }
                Ok(Err(e)) => {
                    error!("Client recv error: {e}");
                }
                Err(_) => {
                    info!("Client: recv timed out (expected for this demo)");
                }
            }

            if let Err(e) = transport.close().await {
                error!("Client close error: {e}");
            }
        }
        Err(e) => {
            info!("Client could not connect: {e}");
        }
    }

    // Clean up
    server.stop();
    info!("Server stopped");

    // Wait a moment for accept task to finish
    let _ = tokio::time::timeout(Duration::from_millis(500), accept_task).await;
}

#[tokio::main]
async fn main() {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("grpc_client_example=info".parse().unwrap())
                .add_directive("mcpkit=debug".parse().unwrap()),
        )
        .init();

    info!("gRPC Transport Example");
    info!("=======================");
    info!("");

    // Demonstrate configuration options
    demonstrate_client_config();
    info!("");

    demonstrate_server_config();
    info!("");

    // Demonstrate connection (will fail if no server is running)
    demonstrate_client_connection().await;
    info!("");

    // Demonstrate server API
    demonstrate_server_api().await;
    info!("");

    info!("Example complete!");
    info!("");
    info!("gRPC transport provides:");
    info!("- Client: GrpcTransport::connect(config).await");
    info!("- Server: GrpcServer::new(config), server.start().await");
    info!("- Accept: server.accept().await for incoming connections");
    info!("- Send/Recv: transport.send(msg) and transport.recv()");
    info!("- Close: transport.close().await, server.stop()");
}

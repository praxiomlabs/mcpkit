# Protocol Version Compatibility

This document describes the MCP protocol versions supported by this SDK and how version negotiation works.

## Supported Protocol Versions

| Version | Status | Description |
|---------|--------|-------------|
| `2025-11-25` | **Latest** | Full feature support, including latest MCP capabilities |
| `2024-11-05` | Supported | Original MCP specification, widely deployed |

## Version Negotiation

Protocol version negotiation happens during the initialization handshake, following the [MCP specification](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle).

### How It Works

1. **Client sends initialize request** with its preferred protocol version:
   ```json
   {
     "jsonrpc": "2.0",
     "id": 1,
     "method": "initialize",
     "params": {
       "protocolVersion": "2025-11-25",
       "capabilities": { ... },
       "clientInfo": { ... }
     }
   }
   ```

2. **Server responds** with:
   - The **same version** if it supports the requested version
   - Its **preferred version** if it doesn't support the requested version

3. **Client validates** the server's response:
   - If the client supports the server's version, proceed
   - If not, the handshake fails

### Best Practices

- **Clients** should send the latest version they support
- **Servers** should support multiple versions for backward compatibility
- Both sides should gracefully handle version mismatches

## Using Version Negotiation

### Server-Side

The SDK automatically handles version negotiation in the server:

```rust
use mcpkit_server::ServerBuilder;
use mcpkit_core::capability::{negotiate_version, SUPPORTED_PROTOCOL_VERSIONS};

// The server automatically negotiates versions during initialization
// You can check which versions are supported:
println!("Supported versions: {:?}", SUPPORTED_PROTOCOL_VERSIONS);
```

### Client-Side

The SDK validates the server's protocol version automatically:

```rust
use mcpkit_client::ClientBuilder;
use mcpkit_core::capability::is_version_supported;

// The client automatically validates the server's version
// You can also check version support programmatically:
assert!(is_version_supported("2025-11-25"));
assert!(is_version_supported("2024-11-05"));
```

### Manual Version Negotiation

For advanced use cases, you can use the negotiation utilities directly:

```rust
use mcpkit_core::capability::{
    negotiate_version,
    negotiate_version_detailed,
    VersionNegotiationResult,
    PROTOCOL_VERSION,
};

// Simple negotiation - returns the negotiated version string
let version = negotiate_version("2024-11-05");
assert_eq!(version, "2024-11-05");

// Detailed negotiation - provides more context
let result = negotiate_version_detailed("unknown-version");
match result {
    VersionNegotiationResult::Accepted(v) => {
        println!("Version {} accepted", v);
    }
    VersionNegotiationResult::CounterOffer { requested, offered } => {
        println!("Client requested {}, server offers {}", requested, offered);
    }
}
```

## Error Handling

When version negotiation fails, the SDK returns appropriate errors:

### Client receives unsupported version

If the server returns a version the client doesn't support:

```rust
use mcpkit_core::error::McpError;

// This error indicates version mismatch
match result {
    Err(McpError::HandshakeFailed(details)) => {
        println!("Version mismatch: {}", details.message);
        println!("Client version: {:?}", details.client_version);
        println!("Server version: {:?}", details.server_version);
    }
    _ => {}
}
```

### JSON-RPC Error Response

Per the MCP specification, version errors use code `-32602`:

```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "error": {
    "code": -32602,
    "message": "Unsupported protocol version",
    "data": {
      "supported": ["2025-11-25", "2024-11-05"],
      "requested": "1.0.0"
    }
  }
}
```

## Compatibility Matrix

### Feature Availability by Version

| Feature | 2024-11-05 | 2025-11-25 |
|---------|------------|------------|
| Tools | Yes | Yes |
| Resources | Yes | Yes |
| Prompts | Yes | Yes |
| Tasks | No | Yes |
| Sampling | Yes | Yes |
| Elicitation | No | Yes |
| Completions | No | Yes |

### SDK Compatibility

| rmcp (Reference Implementation) | mcpkit |
|--------------------------------|--------------|
| 2024-11-05 | Compatible |
| 2025-11-25 | Compatible (SDK native) |

## HTTP Transport Version

The HTTP transport layer uses a separate version header:

```
mcp-protocol-version: 2025-06-18
```

Note: HTTP/2 requires lowercase header names. HTTP/1.1 headers are case-insensitive, so lowercase works universally.

This is the Streamable HTTP transport specification version, which is independent of the core MCP protocol version.

## Future Versions

As new MCP protocol versions are released, this SDK will:

1. Add support for new versions while maintaining backward compatibility
2. Update `SUPPORTED_PROTOCOL_VERSIONS` to include new versions
3. Keep existing versions supported for at least one major release cycle
4. Document any breaking changes or deprecations

## Related Documentation

- [MCP Specification - Lifecycle](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle)
- [Getting Started Guide](./getting-started.md)
- [Error Handling Guide](./error-handling.md)

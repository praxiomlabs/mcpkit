# HTTP MCP Server Example

A full MCP server using HTTP transport with Server-Sent Events (SSE) for streaming responses.

## Running Locally

```bash
cargo run -p http-server-example
```

The server listens on `http://127.0.0.1:3000/mcp`.

## Running with Docker

### Build and run with Docker Compose (recommended)

```bash
cd examples/http-server
docker-compose up --build
```

### Build and run with Docker directly

```bash
# From workspace root
docker build -t mcpkit-http-server -f examples/http-server/Dockerfile .
docker run -p 3000:3000 mcpkit-http-server
```

## Testing the Server

Initialize a session:

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-06-18" \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","clientInfo":{"name":"curl","version":"1.0"}}}'
```

List available tools:

```bash
# Use the session ID from the initialize response
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-06-18" \
  -H "Mcp-Session-Id: <session-id>" \
  -d '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'
```

Call a tool:

```bash
curl -X POST http://localhost:3000/mcp \
  -H "Content-Type: application/json" \
  -H "MCP-Protocol-Version: 2025-06-18" \
  -H "Mcp-Session-Id: <session-id>" \
  -d '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"add","arguments":{"a":2,"b":3}}}'
```

## Configuration

| Environment Variable | Default | Description |
|---------------------|---------|-------------|
| `RUST_LOG` | `info` | Logging level |
| `MCP_BIND_ADDR` | `127.0.0.1:3000` | Server bind address |

For Docker deployments, `MCP_BIND_ADDR` is automatically set to `0.0.0.0:3000`.

## Endpoints

| Method | Path | Description |
|--------|------|-------------|
| POST | `/mcp` | Handle JSON-RPC requests |
| GET | `/mcp` | SSE stream for server-initiated messages |
| DELETE | `/mcp` | Close a session |
| GET | `/health` | Health check endpoint |

## Available Tools

- `add` - Add two numbers
- `subtract` - Subtract b from a
- `multiply` - Multiply two numbers
- `divide` - Divide a by b
- `echo` - Echo back a message
- `get_time` - Get current server time

## Available Resources

- `server://info` - Server information
- `server://status` - Server status

## Available Prompts

- `calculator` - Calculator assistant prompt
- `greeting` - Generate a greeting

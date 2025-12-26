# WebAssembly (WASM) Support

The Rust MCP SDK has experimental support for WebAssembly targets, enabling MCP clients to run in browsers and other WASM environments.

## Supported Targets

| Target | Status | Notes |
|--------|--------|-------|
| `wasm32-unknown-unknown` | Experimental | Core types work, transports limited |
| `wasm32-wasi` | Not tested | May work with additional configuration |

## What Works

### mcpkit-core

The core types and protocol definitions compile for WASM:

- All protocol types (Request, Response, Notification, etc.)
- Error types and handling
- Content types (TextContent, ImageContent, etc.)
- Tool, Resource, and Prompt definitions
- JSON-RPC message encoding/decoding

### What's Limited

Transport implementations have platform-specific dependencies:

| Transport | WASM Support | Notes |
|-----------|--------------|-------|
| Memory | ✅ Works | For testing |
| HTTP | ⚠️ Needs adapter | Use browser fetch API |
| WebSocket | ⚠️ Needs adapter | Use browser WebSocket API |
| Stdio | ❌ N/A | Not applicable in browsers |
| Unix sockets | ❌ N/A | Not available |
| Named pipes | ❌ N/A | Not available |

## Building for WASM

### Prerequisites

```bash
# Install the WASM target
rustup target add wasm32-unknown-unknown

# Optional: Install wasm-pack for web deployment
cargo install wasm-pack
```

### Basic Build

```bash
# Build mcpkit-core for WASM
cargo build -p mcpkit-core --target wasm32-unknown-unknown
```

### Cargo.toml Configuration

For WASM-compatible projects:

```toml
[dependencies]
mcpkit-core = "0.5"
# Don't include transport or server - they have platform dependencies

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.5"
js-sys = "0.3"
web-sys = { version = "0.5", features = ["console"] }
```

## Browser Integration

### Using with JavaScript

```rust
use wasm_bindgen::prelude::*;
use mcpkit_core::protocol::{Message, Request};

#[wasm_bindgen]
pub struct McpClient {
    // Your client state
}

#[wasm_bindgen]
impl McpClient {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {}
    }

    /// Create an MCP request
    #[wasm_bindgen]
    pub fn create_request(&self, method: &str, id: u32) -> String {
        let request = Request::new(method, id);
        let message = Message::Request(request);
        serde_json::to_string(&message).unwrap_or_default()
    }

    /// Parse an MCP response
    #[wasm_bindgen]
    pub fn parse_response(&self, json: &str) -> Result<JsValue, JsValue> {
        let message: Message = serde_json::from_str(json)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        serde_wasm_bindgen::to_value(&message)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
```

### Building with wasm-pack

```bash
wasm-pack build --target web
```

### Using in JavaScript

```javascript
import init, { McpClient } from './pkg/my_mcp_client.js';

async function main() {
    await init();

    const client = new McpClient();

    // Create a request
    const request = client.create_request('tools/list', 1);

    // Send via fetch or WebSocket...
    const response = await fetch('/mcp', {
        method: 'POST',
        body: request,
        headers: { 'Content-Type': 'application/json' }
    });

    // Parse the response
    const json = await response.text();
    const message = client.parse_response(json);
    console.log(message);
}

main();
```

## Custom WASM Transport

For browser environments, you'll need to implement a custom transport using browser APIs:

```rust
use mcpkit_core::protocol::Message;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{Request, RequestInit, Response};

pub struct FetchTransport {
    url: String,
}

impl FetchTransport {
    pub fn new(url: impl Into<String>) -> Self {
        Self { url: url.into() }
    }

    pub async fn send(&self, msg: Message) -> Result<Message, JsValue> {
        let window = web_sys::window().unwrap();

        let body = serde_json::to_string(&msg)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        let mut opts = RequestInit::new();
        opts.method("POST");
        opts.body(Some(&JsValue::from_str(&body)));

        let request = Request::new_with_str_and_init(&self.url, &opts)?;
        request.headers().set("Content-Type", "application/json")?;

        let resp_value = JsFuture::from(window.fetch_with_request(&request)).await?;
        let resp: Response = resp_value.dyn_into()?;

        let text = JsFuture::from(resp.text()?).await?;
        let text_str = text.as_string().unwrap_or_default();

        serde_json::from_str(&text_str)
            .map_err(|e| JsValue::from_str(&e.to_string()))
    }
}
```

## WebSocket Transport for Browsers

```rust
use web_sys::{MessageEvent, WebSocket};
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;

pub struct BrowserWebSocket {
    ws: WebSocket,
}

impl BrowserWebSocket {
    pub fn connect(url: &str) -> Result<Self, JsValue> {
        let ws = WebSocket::new(url)?;
        ws.set_binary_type(web_sys::BinaryType::Arraybuffer);
        Ok(Self { ws })
    }

    pub fn send(&self, msg: &Message) -> Result<(), JsValue> {
        let json = serde_json::to_string(msg)
            .map_err(|e| JsValue::from_str(&e.to_string()))?;
        self.ws.send_with_str(&json)
    }

    pub fn set_onmessage(&self, callback: impl Fn(Message) + 'static) {
        let closure = Closure::wrap(Box::new(move |e: MessageEvent| {
            if let Some(text) = e.data().as_string() {
                if let Ok(msg) = serde_json::from_str::<Message>(&text) {
                    callback(msg);
                }
            }
        }) as Box<dyn Fn(MessageEvent)>);

        self.ws.set_onmessage(Some(closure.as_ref().unchecked_ref()));
        closure.forget(); // Prevent cleanup
    }
}
```

## Known Limitations

1. **No stdio transport**: Browsers don't have stdin/stdout
2. **Async runtime**: You'll need `wasm-bindgen-futures` for async
3. **File system**: No direct file system access (use File API)
4. **Process spawning**: Cannot spawn subprocesses
5. **Network restrictions**: Subject to CORS policies

## Performance Considerations

- WASM binaries can be large; use `wasm-opt` for optimization
- Consider lazy loading MCP functionality
- Use streaming for large resources when possible

## Example Project Structure

```
my-mcp-client/
├── Cargo.toml
├── src/
│   ├── lib.rs          # WASM entry point
│   └── transport.rs    # Browser transport
├── www/
│   ├── index.html
│   └── index.js
└── build.sh            # wasm-pack build script
```

## Troubleshooting

### "wasm32 target may not be installed"

```bash
rustup target add wasm32-unknown-unknown
```

### "getrandom" errors

Ensure `getrandom/js` feature is enabled (done by default in mcpkit-core).

### Large bundle size

Use release builds with LTO:

```toml
[profile.release]
lto = true
opt-level = 's'
```

Run `wasm-opt`:

```bash
wasm-opt -Os -o optimized.wasm pkg/my_client_bg.wasm
```

## See Also

- [wasm-bindgen Documentation](https://rustwasm.github.io/docs/wasm-bindgen/)
- [wasm-pack](https://rustwasm.github.io/docs/wasm-pack/)
- [web-sys API Reference](https://rustwasm.github.io/docs/wasm-bindgen/web-sys/)

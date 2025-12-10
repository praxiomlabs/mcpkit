# ADR 0002: Typestate Pattern for Connection Lifecycle

## Status

Accepted

## Context

MCP connections have a well-defined lifecycle:

```
Disconnected → Connected → Initializing → Ready → Closing → Disconnected
```

Different operations are valid in different states:
- `connect()` is only valid when Disconnected
- `initialize()` is only valid when Connected
- `send_request()` is only valid when Ready
- `shutdown()` is only valid when Ready

Runtime state machines require:
- State tracking field
- Runtime checks on every operation
- Error handling for invalid state transitions
- Testing all state combinations

## Decision

We use the **typestate pattern** to encode connection state in the type system:

```rust
// State marker types (zero-sized)
struct Disconnected;
struct Connected;
struct Initializing;
struct Ready;
struct Closing;

// Connection parameterized by state
struct Connection<S> {
    inner: ConnectionInner,
    _state: PhantomData<S>,
}

// Methods only available in specific states
impl Connection<Disconnected> {
    fn connect(self) -> Connection<Connected> { ... }
}

impl Connection<Connected> {
    fn initialize(self, ...) -> Connection<Initializing> { ... }
}

impl Connection<Ready> {
    fn send_request(&self, ...) -> Response { ... }
    fn shutdown(self) -> Connection<Closing> { ... }
}
```

## Consequences

### Positive

- **Compile-time safety**: Invalid state transitions are compile errors
- **Zero runtime overhead**: Marker types are zero-sized
- **Self-documenting**: Type signature shows required state
- **No runtime checks**: State is proven correct by the type system
- **Better IDE support**: Autocomplete only shows valid methods

### Negative

- **More complex types**: `Connection<Ready>` vs `Connection`
- **Ownership transfer**: State transitions consume `self`
- **Learning curve**: Developers unfamiliar with typestate

### Examples

**Compile-time error prevention:**
```rust
let conn = Connection::new();  // Connection<Disconnected>

// This won't compile - can't send on disconnected connection
// conn.send_request(req);  // ERROR!

let conn = conn.connect();  // Connection<Connected>

// This won't compile - can't send before initialization
// conn.send_request(req);  // ERROR!

let (conn, _) = conn.initialize(info, caps);  // Connection<Initializing>
let conn = conn.complete(server_info, server_caps);  // Connection<Ready>

// Now we can send
conn.send_request(req);  // OK!
```

**Type signatures are documentation:**
```rust
// This function requires a ready connection
fn query_tools(conn: &Connection<Ready>) -> Vec<Tool> {
    // ...
}

// This function transitions connection to closing
fn graceful_shutdown(conn: Connection<Ready>) -> Connection<Closing> {
    // ...
}
```

### Mitigations for Complexity

- Provide type aliases for common states
- Document the state machine clearly
- Include examples in documentation
- Keep the state machine simple (5 states)

## Alternatives Considered

### 1. Runtime State Machine

```rust
enum State { Disconnected, Connected, Initializing, Ready, Closing }

struct Connection {
    state: State,
    // ...
}

impl Connection {
    fn send_request(&self, req: Request) -> Result<Response, StateError> {
        if self.state != State::Ready {
            return Err(StateError::InvalidState);
        }
        // ...
    }
}
```

**Rejected because:**
- Runtime overhead for state checks
- Errors only caught at runtime
- Easy to forget state checks

### 2. Separate Types

```rust
struct DisconnectedConnection { ... }
struct ReadyConnection { ... }
```

**Rejected because:**
- Code duplication
- Harder to share logic
- More types to maintain

## References

- [Typestate Pattern in Rust](https://cliffle.com/blog/rust-typestate/)
- [Session Types](https://aturon.github.io/blog/2015/08/27/epoch/)
- [Encoding State Machines in Rust](https://hoverbear.org/blog/rust-state-machine-pattern/)

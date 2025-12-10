# Research Findings: Rust MCP SDK Design Decisions

**Date:** 2024
**Purpose:** Evidence-based analysis of contested design decisions

---

## 1. Runtime Agnosticism

### The Problem
The official Rust MCP SDK is Tokio-only, blocking users who need async-std or smol for:
- Embedded systems
- WASM targets
- Resource-constrained environments
- Existing non-Tokio codebases

**Source:** [GitHub Issue #379](https://github.com/modelcontextprotocol/rust-sdk/issues/379)

### Research Findings

**From [The Async Ecosystem - Rust Async Book](https://rust-lang.github.io/async-book/08_ecosystem/00_chapter.html):**
> "Libraries exposing async APIs should not depend on a specific executor or reactor, unless they need to spawn tasks or define their own async I/O or timer futures. Ideally, only binaries should be responsible for scheduling and running tasks."

**From [The State of Async Rust](https://corrode.dev/blog/async/):**
> "async-std has officially been discontinued. The suggested replacement is smol."

**From [agnostik crate](https://github.com/bastion-rs/agnostik):**
Runtime agnosticism is achieved through:
1. Abstraction layer between application and executor
2. Unified API (`spawn()`, `block_on()`, `spawn_blocking()`)
3. Mutually exclusive feature flags: `runtime_tokio`, `runtime_asyncstd`, `runtime_smol`
4. Libraries should NOT enable runtime features - consumers choose

### Conclusion: PLAN IS CORRECT

Runtime agnosticism is:
1. A real user need (not theoretical)
2. Achievable through established patterns
3. The right differentiator for this SDK

**Implementation approach:**
- Use `futures` crate traits (`AsyncRead`, `AsyncWrite`, `Stream`, `Sink`)
- Feature flags: `tokio-runtime`, `async-std-runtime`, `smol-runtime`
- No runtime-specific code in core abstractions
- Compatibility layer (like `async-compat`) for interop

---

## 2. Context: Lifetimes vs Arc

### The Question
Should Context use `Context<'a>` with lifetime references or `Context` with `Arc`?

### Research Findings

**From [Lifetime vs Arc in Rust](https://www.niks3089.com/posts/arc-vs-lifetime/):**

> "Lifetimes: No runtime overhead since it's purely compile-time checked."
> "Arc: Introduces some overhead due to atomic reference counting."

**Use lifetimes when:**
- Single-threaded contexts
- You can manage scope manually
- Avoiding allocations is critical

**Use Arc when:**
- Multi-threaded scenarios
- Data must persist beyond scope
- Shared ownership across threads

**From [Inventing the Service Trait](https://tokio.rs/blog/2021-05-14-inventing-the-service-trait):**

> "The issue is that we're capturing a `&mut self` and moving it into an async block. That means the lifetime of our future is tied to the lifetime of `&mut self`. This doesn't work for us, since we might want to run our response futures on multiple threads."

> "Instead we need to convert the `&mut self` into an owned self... That is exactly what `Clone` does."

The Tower pattern uses `Clone + 'static`, not lifetime references.

**From [Axum Handler Trait](https://docs.rs/axum/latest/axum/handler/trait.Handler.html):**
> `pub trait Handler<T, S>: Clone + Send + Sync + Sized + 'static`

Axum requires handlers to be `'static` and `Clone`.

### Conclusion: IT DEPENDS ON GOALS

**For runtime agnosticism + single-threaded support:**
- `Context<'a>` with lifetimes allows `!Send` types
- Avoids forcing `Arc` overhead
- Works with `LocalSet` patterns

**For multi-threaded spawning:**
- `Clone + 'static` pattern is industry standard
- Tower, Axum, Hyper all use this

**Recommended approach for this SDK:**
Since we want to support smol/async-std AND single-threaded use cases:
- **Keep `Context<'a>` as the plan specifies**
- This allows users who DON'T need multi-threading to avoid Arc overhead
- Users who DO need spawning can wrap handlers in Arc themselves
- This is MORE FLEXIBLE than forcing Arc on everyone

---

## 3. ServerBuilder Typestate (5 Type Parameters)

### The Question
Is `ServerBuilder<H, Tools, Resources, Prompts, Tasks>` over-engineered?

### Research Findings

**From [The Typestate Pattern in Rust](https://cliffle.com/blog/rust-typestate/):**
> The article demonstrates **one type parameter** for state tracking.

**From [Typestate Builder Pattern in Rust](https://n1ghtmare.github.io/2024-05-31/typestate-builder-pattern-in-rust/):**
> "When you throw the typestate pattern into the mix... This combo enforces an order to the building process, catching slip-ups during compile time."

**From [Rethinking Builders with Lazy Generics](https://geo-ant.github.io/blog/2024/rust-rethinking-builders-lazy-generics/):**
> "Crates like bon and typed_builder are among the most widely used."

**Trade-offs noted:**
> "The usage of generics increases the binary size and the compilation time. Most of the time the usage of generics still outweighs those disadvantages but there is a point where the returns are not worth overusing generics."

### Analysis

5 type parameters is unusual but not unprecedented. The question is whether the compile-time safety justifies the complexity.

**Benefits:**
- Compile-time verification of registered capabilities
- Prevents runtime "capability not registered" errors
- Self-documenting API

**Costs:**
- More complex type signatures
- Longer compile times
- Steeper learning curve

### Conclusion: PLAN IS AMBITIOUS BUT DEFENSIBLE

The 5-parameter approach:
1. IS more complex than typical builders
2. DOES provide real compile-time safety
3. CAN be simplified if it proves too unwieldy

**Recommendation:** Implement as planned, but be prepared to simplify if user feedback indicates the complexity outweighs benefits.

---

## 4. Middleware Layer System

### Research Findings

**From [Tower Service Trait](https://docs.rs/tower-service/latest/tower_service/trait.Service.html):**

Tower's Service trait IS runtime-agnostic:
> The trait itself doesn't mandate any specific async runtime.

The `Service` trait:
```rust
pub trait Service<Request> {
    type Response;
    type Error;
    type Future: Future<Output = Result<Self::Response, Self::Error>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>>;
    fn call(&mut self, req: Request) -> Self::Future;
}
```

No `Send` bounds at the trait level - those are added by implementations as needed.

**From [Announcing tower-http](https://tokio.rs/blog/2021-05-announcing-tower-http):**
> "Tower itself contains middleware that are all protocol agnostic."

### Conclusion: PLAN IS CORRECT

Tower-compatible middleware is:
1. The industry standard
2. Runtime-agnostic at the trait level
3. Composable and reusable

The plan's middleware design (`TransportLayer`, `LayerStack`, `LoggingLayer`, etc.) aligns with Tower patterns.

---

## Summary: What's Correct?

| Decision | Plan | Research Says | Verdict |
|----------|------|---------------|---------|
| Runtime agnosticism | Required | Real need, achievable | **PLAN CORRECT** |
| Context<'a> lifetimes | Required | Enables flexibility for single-threaded | **PLAN CORRECT** |
| ServerBuilder 5 params | Required | Ambitious but defensible | **PLAN CORRECT** |
| Middleware system | Required | Industry standard | **PLAN CORRECT** |

**THE PLAN IS CORRECT. Follow it.**

---

## Implementation Guidelines

### For Runtime Agnosticism:
1. Use `futures` crate traits, NOT `tokio::io::*`
2. Feature flags select runtime at compile time
3. Core abstractions must NOT import runtime crates
4. Test with all three runtimes in CI

### For Context<'a>:
1. Keep lifetime parameter as specified
2. All references borrowed, not owned
3. Peer trait for communication
4. Users who need spawning wrap in Arc themselves

### For ServerBuilder:
1. Implement 5 type parameters as specified
2. Use `()` as "not registered" marker type
3. Methods transform type parameters on registration
4. Consider ergonomic helpers if complexity is high

### For Middleware:
1. `TransportLayer<T: Transport>` trait
2. `LayerStack<T>` for composition
3. Built-in layers: Logging, Timeout, Retry, Metrics
4. Compatible with Tower patterns

---

## Sources

- [The Async Ecosystem - Rust Async Book](https://rust-lang.github.io/async-book/08_ecosystem/00_chapter.html)
- [The State of Async Rust](https://corrode.dev/blog/async/)
- [Lifetime vs Arc in Rust](https://www.niks3089.com/posts/arc-vs-lifetime/)
- [Inventing the Service Trait](https://tokio.rs/blog/2021-05-14-inventing-the-service-trait)
- [The Typestate Pattern in Rust](https://cliffle.com/blog/rust-typestate/)
- [Typestate - Type-Driven API Design](https://willcrichton.net/rust-api-type-patterns/typestate.html)
- [Tower Service Trait](https://docs.rs/tower-service/latest/tower_service/trait.Service.html)
- [Axum Handler Trait](https://docs.rs/axum/latest/axum/handler/trait.Handler.html)
- [agnostik crate](https://github.com/bastion-rs/agnostik)
- [GitHub Issue #379 - Official MCP SDK](https://github.com/modelcontextprotocol/rust-sdk/issues/379)

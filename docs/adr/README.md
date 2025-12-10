# Architecture Decision Records

This directory contains Architecture Decision Records (ADRs) for the Rust MCP SDK.

## What is an ADR?

An Architecture Decision Record captures an important architectural decision made along with its context and consequences.

## ADR Index

| ADR | Title | Status |
|-----|-------|--------|
| [0001](0001-unified-macro-system.md) | Unified Macro System | Accepted |
| [0002](0002-typestate-pattern.md) | Typestate Pattern for Connection Lifecycle | Accepted |
| [0003](0003-unified-error-type.md) | Unified Error Type | Accepted |
| [0004](0004-runtime-agnostic-design.md) | Runtime-Agnostic Design | Accepted |

## Creating a New ADR

1. Copy the template below
2. Create a new file: `NNNN-title-with-dashes.md`
3. Fill in the sections
4. Submit for review

## Template

```markdown
# ADR NNNN: Title

## Status

[Proposed | Accepted | Deprecated | Superseded]

## Context

What is the issue that we're seeing that is motivating this decision or change?

## Decision

What is the change that we're proposing and/or doing?

## Consequences

What becomes easier or more difficult to do because of this change?

## Alternatives Considered

What other options were considered and why were they rejected?

## References

- Links to relevant resources
```

## References

- [ADR GitHub Organization](https://adr.github.io/)
- [Michael Nygard's ADR article](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)

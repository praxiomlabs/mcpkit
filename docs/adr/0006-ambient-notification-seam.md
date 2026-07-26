# ADR 0006: Ambient Notification Seam

## Status

Accepted

## Context

Implementing `notifications/tasks/status` surfaced a structural gap that had
nothing to do with tasks.

Everything the server sent to the client had to originate inside a future that
already held a request-scoped [`Peer`], and `Peer`s were handed out per request
through `Context`. There was no path for an **ambient** state change — one that
no inbound request triggered — to reach the client at all.

`notifications/tasks/status` was simply the first case to surface it. The queue
behind it is real: resource-subscription updates driven by a file watcher,
dynamic tool-list changes, background progress, log forwarding. Every one of
them poses the same question.

The store side had a matching gap. `TaskManager` is a synchronous state store in
`mcpkit-core` with no change-notification surface, except a per-task terminal
`event_listener::Event` bolted on solely for `wait_terminal` — the same problem
already solved once, narrowly and ad hoc.

The two sides could not be joined directly. `TaskManager`'s transition points
(`set_status`, `finish`, `cancel`) are synchronous and hold an `RwLock`, while
`Peer::notify` is async and lives in `mcpkit-server`; core cannot reference it
without inverting the crate dependency.

The server's only drain point is `ServerRuntime::run`'s
`select(recv, drive_sets(in_flight, background, notifications))` loop, where each
`FuturesUnordered` is generic over a single concrete future type. Adding a source
therefore meant changing `drive_sets`' signature and the loop's types — which is
why each new async concern had been expensive enough to avoid.

## Decision

Add one seam on each side, and make each generic rather than task-specific.

**Core emits domain events, not protocol messages.** `TaskManager` gained
`TaskEvent { task, previous_status }` and a `TaskObserver` trait, fired on every
transition. The event is a domain fact: the store knows nothing about
`notifications/tasks/status`, peers, or the wire. The crate dependency direction
is unchanged, and the same stream serves metrics, recording, and tests.

Each transition builds its event under the store lock and fires the observer
after releasing it, so an observer may call back into the same `TaskManager`
without deadlocking.

**The server gained an ambient notification pump.** `ServerState` holds an
`UnboundedSender<Notification>` behind `publish_notification()` — a synchronous,
non-blocking enqueue, safe to call from a lock-free callback. The run loop takes
the drain end once and races it against `recv` and `drive_sets`, writing each
notification inline: a notification is one small frame, so that costs less than
carrying a fourth future set through `drive_sets`.

Mapping between the two is a small server-side adapter (`TaskStatusNotifier`),
the only place that decides a transition is worth telling the client about. It
is parameterized over a `NotificationSink` rather than bound to this pump, so
the same mapping serves transports that reach their client differently.

The property that matters: **the loop grows one drain, not one arm per feature.**
Later ambient sources reuse it without touching `run()` again.

## Consequences

- New ambient notification sources cost zero changes to the run loop.
- Delivery is best-effort: a failed write is logged, never fatal to the session.
  This matches the spec's treatment of notifications, and `tasks/status` in
  particular is optional ("Receivers MAY send"). `RuntimeConfig::task_status_notifications`
  gates it, defaulting on.
- The HTTP adapters have no run loop and never construct a `ServerState`, so the
  pump does not reach them. They were addressed by generalizing the destination
  rather than adding a second notifier: `NotificationSink` is implemented for
  both `ServerState` (this pump) and `StreamRegistry` (store-and-forward onto the
  session's SSE stream), and `session_task_store` is the single place a
  per-session store is wired to its registry. The transition-to-notification
  mapping is therefore written once for every transport.
- Delivery there is still best-effort: with no live SSE stream the event is
  dropped, and a client that never reconnects misses it.
- `TaskManager` gained public API (`TaskEvent`, `TaskObserver`, `set_observer`),
  which is semver-relevant on a crate heading to 1.0.
- Server tests must tolerate notifications interleaving with responses; the
  `next_response` helper now skips them.

## Where this leads: normalizing the run loop

This ADR deliberately stops short of the end state. `ServerRuntime::run` is still
a hand-rolled `select` over heterogeneous concrete future types, and the pump is
bolted to the side of it rather than being one source among equals.

The shape the loop wants is a single unified event enum — inbound message,
outbound notification, background completion, timer — over boxed streams via
`select_all`, replacing `drive_sets` entirely. That would pay down the loop debt
rather than routing around it.

It was not done here because it rewrites the most delicate, most-commented
concurrency code in the crate, and it deserves its own change with its own
review rather than arriving as a side effect of adding one notification.

The seam above is a strict subset of that design: the pump is the first
normalized source, so this decision does not pre-empt the refactor. When the loop
is normalized, `publish_notification` becomes one producer among several and its
callers do not change.

## Alternatives considered

**Emit at the server's existing transition points** (`run_task`, the
`tasks/cancel` route). No core change and no loop change, but it covers only
runtime-driven transitions — a tool calling `handle.mark_input_required()` would
emit nothing — and the next ambient source pays the same cost again. Rejected as
booking the debt rather than paying it.

**Normalize the whole loop now.** The correct end state, deferred for the reasons
above.

**A channel in core.** Would have put an async dependency in a crate that has
none, and `futures` had just been removed from `mcpkit-core` as unused. The
sync-observer/async-drain split keeps core free of async plumbing and lets the
server choose the mechanism.

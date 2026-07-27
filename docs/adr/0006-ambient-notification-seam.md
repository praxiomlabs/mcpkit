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

## Where this led: normalizing the run loop

*Resolved 2026-07-27. This section originally proposed a follow-up refactor;
what follows is what actually happened, kept because the reasoning matters more
than the conclusion.*

The original text said the loop wanted "a single unified event enum ... over
boxed streams via `select_all`, replacing `drive_sets` entirely", deferred
because it rewrites the most delicate concurrency code in the crate.

**That design does not work.** `SelectAll` is homogeneous and its `push` adds
*streams*, not futures into a member stream; the loop must push new futures
into `in_flight` and `background` between iterations, and `iter_mut` hands back
`&mut St` with no way to recover the `FuturesUnordered`.

**The refactor is also no longer needed.** It was wanted for two things:

1. *Fairness.* `futures::future::select` polls its left argument first and
   returns the moment it is ready, so a source pinned to the right runs only
   when everything left of it is pending. The pump was added as the rightmost
   arm, which made it the lowest-priority source in the loop: a notification
   published before the loop even started was delivered behind 200 queued
   responses. `progress` sat right of `recv` and starved the same way, so
   during an inbound burst no request completed until the burst drained.

   Both are fixed by rotating which source gets first refusal, and regression
   tested in both directions. The loop is fair by construction.

2. *Cheap new sources.* Already delivered by this ADR's own pump, and the claim
   above ("the loop grows one drain, not one arm per feature") was right. Every
   ambient source this ADR anticipated — resource-subscription updates,
   tool-list changes, background progress, log forwarding — reaches the client
   through `NotificationSink` -> `publish_notification` -> the existing drain.
   None adds a select arm. The three sources the loop races (inbound transport,
   in-flight progress, ambient drain) are structural, not feature-driven, so
   there is no growth pressure on the arm count.

What the deferral did cost was the *fairness* half of "one source among
equals" — the pump was one source among equals structurally, but last in poll
order behaviourally, and nothing tested it. That gap survived until the loop
was probed directly.

An attempt to collapse the three rotation arms into an array of
`Pin<&mut dyn Future>` was abandoned: type erasure drops `Send`, so `run()`
stops being spawnable, and `+ Send` is not provable generically because
`RequestRouter` does not declare `Send` futures. Putting `+ Send` on a public
trait's return types is out of proportion to tidying three match arms.

If this is reopened, the trigger should be a real fourth *structural* source,
and the design should start from a generic `Arm<A, B, C>` future — which
preserves `Send` — not from the `select_all` sketch above.

## Alternatives considered

**Emit at the server's existing transition points** (`run_task`, the
`tasks/cancel` route). No core change and no loop change, but it covers only
runtime-driven transitions — a tool calling `handle.mark_input_required()` would
emit nothing — and the next ambient source pays the same cost again. Rejected as
booking the debt rather than paying it.

**Normalize the whole loop now.** Deferred at the time, and later closed as
superseded — see "Where this led" above. It was not the correct end state: the
shape proposed for it could not be built.

**A channel in core.** Would have put an async dependency in a crate that has
none, and `futures` had just been removed from `mcpkit-core` as unused. The
sync-observer/async-drain split keeps core free of async plumbing and lets the
server choose the mechanism.

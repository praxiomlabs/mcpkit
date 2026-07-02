# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Logging utility, now functional (#108). New core types `LoggingLevel` (the
  eight syslog levels, ordered by severity), `SetLevelRequest`, and
  `LoggingMessageNotificationParams { level, logger?, data, _meta? }`. Inbound
  `logging/setLevel` is now dispatched (via a shared `route_logging` helper, so it
  works the same in the runtime router *and* the four HTTP adapters) to a new
  `ServerHandler::set_log_level` (default no-op), but only when the `logging`
  capability is advertised; outbound
  logs are emitted via `ServerNotifier::log(level, logger, data)` and the
  `Context::log(..)` convenience (both serialize the typed params). **Fixed:** the
  advertised `logging` capability was entirely non-functional — `logging/setLevel`
  returned method-not-found and there was no way to emit `notifications/message`.
  **Breaking:** the orphaned `LoggingHandler` trait is removed (its `set_level`
  moved onto `ServerHandler` as `set_log_level`), and `mcpkit_server::LogLevel` is
  now a re-export of the core `LoggingLevel`
  ([#108](https://github.com/praxiomlabs/mcpkit/issues/108)).
- Resource subscription request types + wiring (#107). New core
  `SubscribeRequest`/`UnsubscribeRequest` (`{ uri }`), and the server now actually
  dispatches `resources/subscribe`/`resources/unsubscribe` to
  `ResourceHandler::subscribe`/`unsubscribe` (added to the object-safe dispatch
  layer). **Fixed:** these methods previously fell through routing and returned
  `method not found` despite the advertised `resources.subscribe` capability. A
  handler returning `true` yields the spec's empty result; `false` is an error
  (the subscription was not established) rather than a false success. The client's
  `subscribe_resource`/`unsubscribe_resource` now use the typed requests
  ([#107](https://github.com/praxiomlabs/mcpkit/issues/107)).
- Typed progress/cancelled notification params (`mcpkit-core`, #112).
  `ProgressNotificationParams { progressToken, progress: f64, total?, message?,
  _meta? }` and `CancelledNotificationParams { requestId?, reason?, _meta? }`
  replace the ad-hoc raw-JSON handling. The server's `notifications/cancelled`
  handler now parses the typed struct, and `Context::progress` serializes the
  typed struct (omitting absent `total`/`message` instead of emitting `null`).
  Clients gain `ClientHandler::on_progress(ProgressNotificationParams)` and
  `Client::request_with_progress(method, params, token)` (attaches
  `_meta.progressToken` via the new `Meta::with_progress_token_in_params`) to opt
  into progress for a call. **Fixed:** incoming `notifications/progress` were
  dropped for numeric progress tokens and mis-parsed as `TaskProgress`; they now
  route to `on_progress` with typed params. **Breaking:** `Context::progress` now
  takes `f64` (was `u64`), matching the spec's fractional `progress`
  ([#112](https://github.com/praxiomlabs/mcpkit/issues/112)).
- Protocol `_meta` foundation (`mcpkit-core`). A new `Meta` type (an open,
  string-keyed map, `types::meta`) models the MCP `_meta` object, with typed
  `progress_token`/`with_progress_token` accessors bridging `ProgressToken` and a
  `Meta::progress_token_from_params` helper (the server's hand-rolled progress-
  token extraction now delegates to it). Concrete result structs
  (`CallToolResult`, `GetPromptResult`, `ListToolsResult`, the other `*Result`
  types, `InitializeResult`, `PingResult`) gain an optional `meta: Option<Meta>`
  field, serialized as `_meta` and omitted when absent. The base `Task` is left
  spec-pure (no `_meta`); task-get/cancel result and status-notification `_meta`
  (spec `Result & Task`) is deferred to
  [#136](https://github.com/praxiomlabs/mcpkit/issues/136) to avoid contaminating
  nested `Task` values. A generic `Request<T>` refactor was deliberately avoided;
  request-param `_meta` beyond `progressToken` is handled at the boundary rather
  than per-struct.
  **Breaking:** the new field means struct-literal construction of these result
  types now requires `meta` (use the existing constructors, `..Default::default()`
  where derived, or `meta: None`)
  ([#109](https://github.com/praxiomlabs/mcpkit/issues/109)).
- Redaction hook for transport content logging (`mcpkit-transport`). When
  `LoggingLayer::with_contents(true)` logs full messages, secrets could leak into
  logs. `LoggingLayer` gains `redact_keys(keys)` (recursively masks object values
  under case-insensitively-matching keys with `<redacted>`), `redact_with(fn)` (a
  custom hook returning the JSON to log), and `with_redacted_contents()` (enables
  content logging with the new `LoggingLayer::DEFAULT_REDACT_KEYS` denylist
  applied). Redaction only affects the logged representation — the forwarded
  message is never modified — and is skipped entirely when content logging is off;
  a serialization failure logs a placeholder rather than the raw message. Default
  behaviour is unchanged (no redactor unless configured)
  ([#84](https://github.com/praxiomlabs/mcpkit/issues/84)).
- Opt-in tool I/O schema validation (`mcpkit-server`, feature
  `schema-validation`, off by default). A new `ValidatingToolHandler<H>` decorator
  validates `tools/call` arguments against each tool's `inputSchema` and
  structured results against its `outputSchema`, resolving schemas from the inner
  handler's `list_tools` at call time (no cache). Enable it on the builder with
  `ServerBuilder::validate_tool_io()` (or the granular `validate_tool_inputs()` /
  `validate_tool_outputs()`); HTTP-adapter users can wrap their handler directly
  (the decorator transparently forwards `ServerHandler`/`ResourceHandler`/
  `PromptHandler`). Per the Tools spec's error handling, arguments that fail the
  `inputSchema` yield an `isError: true` result (not a `-32602` protocol error,
  which stays reserved for malformed envelopes and unknown tools); an
  `outputSchema` violation is logged as a server bug, the invalid
  `structuredContent` is dropped, and the call returns `isError: true`. The
  generic `ToolHandler::call_tool` path remains an unchecked escape hatch unless
  wrapped. String `format` assertions are not enforced. A standalone
  `validate_json` helper is also exported
  ([#85](https://github.com/praxiomlabs/mcpkit/issues/85)).
- List pagination (opt-in). A new `mcpkit_core::pagination` module provides an
  opaque, versioned base64url cursor codec and a `paginate` helper. The server
  now honours the inbound `cursor` and returns a `nextCursor` for `tools/list`,
  `resources/list`, `resources/templates/list`, and `prompts/list`.
  `Server::list_page_size(n)` enables paging at page size `n` (default: disabled
  — lists return everything with no `nextCursor`, unchanged behaviour); an
  invalid cursor is an `Invalid params` error. Handler traits are unchanged
  (routing-layer paging). Cursors are offset-based, so a list that changes
  between page requests may skip or repeat entries — fine for static tool/prompt
  lists, a documented caveat for dynamic resource lists. The HTTP adapters
  (axum/actix/warp/rocket) expose the same configuration via
  `McpRouter::list_page_size(n)` (and `McpState::with_list_page_size(n)`). On the
  client side, `Client::list_tools`, `list_resources`, `list_resource_templates`,
  and `list_prompts` now follow `nextCursor` to exhaustion rather than silently
  truncating to the first page against a paginating server (a server that returns
  a non-advancing cursor is rejected instead of looped on); the single-page
  `*_paginated` variants are unchanged
  ([#78](https://github.com/praxiomlabs/mcpkit/issues/78)).
- URL-mode elicitation (2025-11-25), core + server. `ElicitRequest` gains an
  optional `mode` (absent = form); new `UrlElicitRequest`
  (`mode`/`message`/`elicitationId`/`url`), `ElicitRequestParams` (a union that
  parses either mode from the wire), and `ElicitationCompleteNotification`.
  `Context::elicit_url` sends a URL-mode `elicitation/create` (gated on the
  client's `elicitation.url` sub-capability), and `ServerNotifier::elicitation_complete`
  sends `notifications/elicitation/complete` for the out-of-band completion.
  `McpError::url_elicitation_required` carries the `-32042`
  `URLElicitationRequired` error with its `data.elicitations` preserved on the
  wire. `ClientCapabilities` gains `with_form_elicitation`/`with_url_elicitation`
  and `has_form_elicitation`/`has_url_elicitation`; `ElicitationCapability` is now
  `{ form?, url? }` (an empty `{}` remains form-capable for backwards
  compatibility).
  The client now handles both modes: it parses `elicitation/create` via the
  `ElicitRequestParams` union and dispatches URL-mode requests to a new
  `ClientHandler::elicit_url` (default: **decline** — a client must not open a
  URL without explicit user consent), and dispatches
  `notifications/elicitation/complete` to `ClientHandler::on_elicitation_complete`.
  The `#[mcp_client]` macro gains `#[elicit_url]` and `#[on_elicitation_complete]`
  handler attributes
  ([#103](https://github.com/praxiomlabs/mcpkit/issues/103)).
- Session-to-verified-user binding (security). A new
  `mcpkit_core::auth::VerifiedUser` (`subject` + `issuer` + `audience`) models an
  identity verified from an access token, and `check_session_binding` enforces
  that a session bound to a user is only ever used by that same user (identity is
  the `(issuer, subject)` pair; audience is recorded but not part of equality —
  validate it at the token boundary). Each adapter session store
  (`mcpkit-axum`/`mcpkit-actix`/`mcpkit-warp`/`mcpkit-rocket`) gains
  `create_for_user(..)` plus `touch_verified(..)` (and `get_verified(..)` on
  axum/actix) that reject a mismatched, missing, or unexpectedly-present identity;
  the adapter `Session` carries an `Option<VerifiedUser>`. Token validation stays
  pluggable (use the JWT helpers + `VerifiedUser::from_claims`); this is the
  store-level mechanism, with adapter handler wiring (POST + SSE) to follow
  ([#86](https://github.com/praxiomlabs/mcpkit/issues/86)).
- Session binding is now enforced by the adapter handlers (all four:
  `mcpkit-axum`/`mcpkit-actix`/`mcpkit-warp`/`mcpkit-rocket`), on both the POST
  and SSE paths. The handlers read the request's `VerifiedUser` (axum/actix from
  request extensions; rocket via a `VerifiedUserGuard` reading request-local
  cache; warp as a filter-supplied argument — the default router passes anonymous,
  so auth-aware apps compose their own filter), bind a new session to it, and
  reject a request that presents a mismatched, missing, or unexpectedly-present
  identity — including before replaying buffered SSE events to a reconnecting
  client ([#86](https://github.com/praxiomlabs/mcpkit/issues/86)).
- Structured tool output (MCP `outputSchema` / `structuredContent`): `Tool` gains
  an `output_schema` field + `Tool::output_schema(..)`, and `CallToolResult` gains
  a `structured_content` field + `CallToolResult::with_structured_content(..)`.
- `Json<T>` tool-return wrapper: a `#[tool]` method may return `Json(value)` (or
  `Result<Json<T>, McpError>`) to populate the result's `structuredContent` from
  a serializable `T`, with a pretty-printed JSON text fallback in `content`. Tool
  methods may now return any type that converts into `ToolOutput`.
- A `#[tool]` returning `Json<T>` now also advertises the tool's `outputSchema`,
  derived from `T` (which should derive `ToolInput` in addition to `Serialize`).
- `SessionStore::with_init_timeout` and `Session::is_reapable` in the
  `mcpkit-axum` and `mcpkit-actix` adapters, plus a `DEFAULT_INIT_TIMEOUT`
  constant, to bound how long a session created but never initialized is kept.
- `SessionStore::with_idle_timeout` and a `DEFAULT_SESSION_TIMEOUT` constant in
  the `mcpkit-warp` and `mcpkit-rocket` adapters, to configure the idle timeout
  used when reaping inactive sessions.
- `ResourceLink` tool-result content type: a `Content::ResourceLink` variant
  (serialized as `"resource_link"`) backed by a new `ResourceLinkContent`
  (`uri`, `name`, optional `description`/`mimeType`/`annotations`), plus a
  `Content::resource_link(uri, name)` constructor and `Content::is_resource_link`,
  so tools can return a resource handle to fetch instead of inlining the payload
  ([#80](https://github.com/praxiomlabs/mcpkit/issues/80)).
- `ServerNotifier`, a cloneable handle obtained from `ServerRuntime::notifier()`
  for sending server-initiated notifications from outside a request context.
  It exposes `tools_list_changed()`, `resources_list_changed()`,
  `prompts_list_changed()`, `resource_updated(uri)`, and a generic `notify()`,
  so a server can tell the client to re-list when its tool/resource/prompt set
  changes between requests ([#77](https://github.com/praxiomlabs/mcpkit/issues/77)).
- Server-initiated request/response: `Context::request(method, params)` lets a
  handler send a request to the client and await its response (the basis for
  elicitation and sampling), and `Peer` gains a `request` method (default-erroring
  so existing implementors are unaffected). The runtime correlates the response,
  bounds the wait by `RuntimeConfig::outbound_request_timeout`, and aborts if the
  request's context is cancelled. Receiving these responses no longer starves the
  message loop at the concurrency limit
  ([#73](https://github.com/praxiomlabs/mcpkit/issues/73)).
- `Context::elicit(ElicitRequest) -> ElicitResult` for form-mode elicitation: a
  handler can request structured input from the user through the client and
  await the result. Gated on the client's `elicitation` capability and the
  negotiated protocol version ([#73](https://github.com/praxiomlabs/mcpkit/issues/73)).
- `Context::create_message(CreateMessageRequest) -> CreateMessageResult` for
  sampling: a handler can ask the client to run an LLM completion and await the
  result. Gated on the client's `sampling` capability
  ([#73](https://github.com/praxiomlabs/mcpkit/issues/73)).
- `mcpkit_transport::http::OriginValidator` for `Origin`-header validation, and
  `McpRouter::with_allowed_origins(..)` / `McpRouter::allow_any_origin()` in the
  `mcpkit-axum`, `mcpkit-actix`, `mcpkit-rocket`, and `mcpkit-warp` adapters to configure it
  ([#82](https://github.com/praxiomlabs/mcpkit/issues/82)).
- 2025-11-25 display metadata and tool execution fields. A shared `Icon` type
  (`src`, `mimeType`, `sizes`, `theme`) plus `IconTheme`, and the `title`/`icons`
  fields the spec adds via `BaseMetadata`/`Icons`: `Tool`, `Resource`,
  `ResourceTemplate`, and `Prompt` gain `title` + `icons`; `PromptArgument` gains
  `title`; `ServerInfo`/`ClientInfo` gain `title` + `icons`. `Tool` also gains
  `execution: Option<ToolExecution>` with `ToolExecution.task_support`
  (`TaskSupport::{Forbidden, Optional, Required}`, default `Forbidden`) for
  task-augmented execution negotiation. Each type has builder setters, and the
  `#[tool(..)]` macro accepts `title = ".."` and `task_support = "optional"`
  (a `task_support` value other than `"forbidden"`/`"optional"`/`"required"` is a
  compile error). **Breaking:** these structs have public fields, so code that
  constructs them with a struct literal must add the new fields
  ([#79](https://github.com/praxiomlabs/mcpkit/issues/79)).
- MCP Tasks dispatch and capability advertisement. `ServerBuilder::with_tasks`
  now advertises the `tasks` capability (previously it registered the handler
  but left the capability unadvertised), and the server routes `tasks/list`,
  `tasks/get`, and `tasks/cancel` to the registered `TaskHandler` via a new
  `mcpkit_server::router::route_tasks` (`tasks/get`/`tasks/cancel` return the
  task; unknown task ids are reported as errors). Wired through the consolidated
  slot dispatch (a `DynTaskHandler`/`TaskSlot` pair). `tasks/result` and the
  task-augmented `tools/call` flow are the next step; task dispatch on the
  framework adapters is a follow-up
  ([#81](https://github.com/praxiomlabs/mcpkit/issues/81)).
- Task-augmented `tools/call`. A `tools/call` whose params include a `task`
  object now runs as a task when the named tool declares
  `execution.taskSupport` of `optional`/`required` (a `forbidden`/absent tool
  rejects the augmentation): the server replies with `CreateTaskResult`
  immediately and runs the tool in the background, off the request-concurrency
  limit, then stores the result. The `ServerRuntime` owns a built-in task store
  and serves `tasks/list`/`tasks/get`/`tasks/cancel`/`tasks/result` from it
  (falling through to a custom `with_tasks` handler for ids it does not own);
  `tasks/result` returns the tool's `CallToolResult` once `completed`. The
  task's cancel token is wired into the execution context, so `tasks/cancel`
  aborts the running tool. The `#[mcp_server]` macro auto-advertises the `tasks`
  capability when any `#[tool]` declares `task_support` other than `forbidden`
  ([#81](https://github.com/praxiomlabs/mcpkit/issues/81)).

### Changed

- **Breaking (behavior):** the HTTP adapters now reject a request that presents an
  `mcp-session-id` for a session the server does not know (previously such a
  request was accepted and processed as if freshly created). Clients must use a
  session id the server minted, or omit the header to start a new session. This
  tightens session semantics alongside the user-binding work
  ([#86](https://github.com/praxiomlabs/mcpkit/issues/86)). The adapter POST
  handlers also gain a verified-user parameter/guard (`Option<Extension<VerifiedUser>>`
  on axum, `VerifiedUserGuard` on rocket, an `Option<VerifiedUser>` filter argument
  on warp), which changes their signatures.
- **Breaking:** removed the non-functional, inverted server-side scaffolding for
  elicitation and sampling — the `ElicitationHandler`/`SamplingHandler` traits
  and the `ElicitationService`/`SamplingService` builders. They modelled the
  server *answering* its own elicitation/sampling, which is backwards from the
  spec (the server *sends* the request to the client); use `Context::elicit` and
  `Context::create_message` instead.
- **Breaking:** `RuntimeConfig` gains an `outbound_request_timeout` field
  (construct with `..RuntimeConfig::default()` if you build it with a struct
  literal).
- **Breaking:** `Session::mark_initialized` in the `mcpkit-axum` and
  `mcpkit-actix` adapters now takes the negotiated `ProtocolVersion` in addition
  to the client capabilities, and `Session` gains a `protocol_version` field.
- `ConnectionInner` (`mcpkit-core`) and `ConnectionData` (`mcpkit-server`) are
  now `#[doc(hidden)]`. They are internal implementation details of
  `Connection` and were never intended as stable API.
- **Breaking:** the task types are rewritten to the 2025-11-25 spec model. `Task`
  is now `{ taskId, status, statusMessage?, createdAt, lastUpdatedAt, ttl,
  pollInterval? }`; `TaskStatus` is `working`/`input_required`/`completed`/
  `failed`/`cancelled` (was `pending`/`running`/…). The non-spec `TaskError`,
  `TaskSummary`, and the `Task` fields `tool`/`progress`/`result`/`error`/
  `updatedAt` are removed; `id` is now `taskId`. New spec types: `TaskMetadata`
  (the request `task` augmentation field), `CreateTaskResult`,
  `GetTaskRequest`/`GetTaskResult`, `GetTaskPayloadRequest`/`GetTaskPayloadResult`
  (`tasks/result`), `CancelTaskRequest`/`CancelTaskResult`, `ListTasksRequest`/
  `ListTasksResult`. `ListTasksRequest` no longer carries a `status` filter (not
  in the spec). The server's `TaskManager`/`TaskHandle` are reworked to the new
  model (a task is `working` on create, stores a payload on `complete`).
  `TaskProgress` is retained for the progress-notification handler pending #112.
  This is the type layer for [#81](https://github.com/praxiomlabs/mcpkit/issues/81);
  capability advertisement and the task-augmented `tools/call` flow follow.
- Server request dispatch is consolidated. The combinatorial
  `impl_request_router!` macro (one `RequestRouter` impl per registered-handler
  combination) is replaced by a single slot-based impl backed by a new
  `mcpkit_server::dispatch` module (object-safe `DynToolHandler`/
  `DynResourceHandler`/`DynPromptHandler` + `ToolSlot`/`ResourceSlot`/
  `PromptSlot`). The duplicate per-method routing in `server.rs` is removed; the
  single source of truth is `mcpkit_server::router` (`route_tools`/
  `route_resources`/`route_prompts` now take `&dyn Dyn*Handler`), shared by the
  server runtime and the framework adapters. Behavior is unchanged. **Breaking
  (minor):** the `route_*` functions' signatures changed from generic
  `<H: ToolHandler>(&H, ..)` to `(&dyn DynToolHandler, ..)` — a concrete `&handler`
  reference still coerces in, so most call sites are unaffected
  ([#117](https://github.com/praxiomlabs/mcpkit/issues/117)).

### Removed

- The standalone `HttpTransportListener` (and `HttpServerConfig`) in
  `mcpkit-transport` — a non-functional Streamable HTTP server stub that echoed
  requests, never routed to a handler, and had placeholder SSE/DELETE. It could
  not serve MCP and duplicated the framework adapters. Serve Streamable HTTP with
  a framework adapter (`mcpkit-axum`, `mcpkit-actix`, `mcpkit-warp`,
  `mcpkit-rocket`) instead. The HTTP *client* (`HttpTransport`,
  `HttpTransportConfig`, `HttpTransportBuilder`) and `OriginValidator` are
  unchanged ([#83](https://github.com/praxiomlabs/mcpkit/issues/83)).

### Fixed

- The client now dispatches elicitation requests on the spec method name
  `elicitation/create` instead of `elicitation/elicit`, so requests from
  spec-compliant servers are handled
  ([#88](https://github.com/praxiomlabs/mcpkit/issues/88)).
- Request cancellation is now wired end to end: the server registers each
  request's cancellation token while it runs, so a `notifications/cancelled`
  for that request id trips the handler's `ctx` (`is_cancelled()` /
  `cancelled()`). Previously the context held an unregistered token, so cancel
  notifications had no effect; numeric request ids are now matched as well
  ([#87](https://github.com/praxiomlabs/mcpkit/issues/87)).
- **Breaking (wire format):** embedded resource content (`Content::Resource`)
  now nests its payload under a `resource` key to match the spec's
  `EmbeddedResource` (`{ "type": "resource", "resource": { "uri": .. } }`).
  Previously the `uri`/`mimeType`/`text`/`blob` fields were hoisted to the top
  level, which spec-compliant peers could not parse. `ResourceContent` now holds
  a single `resource: ResourceContents` field plus `annotations`
  ([#106](https://github.com/praxiomlabs/mcpkit/issues/106)).

### Security

- **Breaking (behavior):** the `mcpkit-axum`, `mcpkit-actix`, `mcpkit-rocket`,
  and `mcpkit-warp` adapters now validate the request `Origin` header to defend
  against DNS-rebinding attacks, and **reject non-loopback browser origins by
  default**
  (previously all origins were accepted). Loopback origins and requests without
  an `Origin` header (non-browser clients) are still allowed; add production
  origins with `McpRouter::with_allowed_origins([..])`, or opt out with
  `allow_any_origin()` ([#82](https://github.com/praxiomlabs/mcpkit/issues/82)).
- OAuth/token types (`TokenResponse`, `TokenRequest`, `AuthorizationConfig`,
  `PkceChallenge`, `ClientRegistrationResponse`) now redact their secret fields
  in `Debug` output. Previously deriving `Debug` printed access/refresh tokens,
  client secrets, authorization codes, and PKCE verifiers verbatim, which could
  leak them into logs or traces; the secret fields now render as `<redacted>`
  while non-secret metadata and `Some`/`None` presence stay visible.

### Fixed

- Generated tool input schemas now emit their `properties` in a deterministic
  order. The `#[mcp_server]` macro previously collected properties through a
  `HashMap`, so `tools/list` returned schemas whose property order varied per
  run — breaking response caching and snapshot tests. Properties are now
  inserted in declaration order.
- A malformed message on the stdio transport no longer tears down the
  connection. `StdioTransport` now replies with a JSON-RPC parse error
  (`-32700`, `id: null`) and keeps serving, instead of returning a transport
  error that ended the server's message loop.
- `ping` is now answered before the `initialize` handshake completes, instead of
  being rejected with "Server not initialized". Other requests still require
  initialization first.
- `CallToolResult` now deserializes when the `content` field is absent (defaults
  to empty), so results carrying only `isError` (or `{}`) from other peers parse
  instead of failing.
- The HTTP client now delivers JSON-RPC error bodies returned with a non-2xx
  status as responses to the awaiting request, instead of failing the whole
  transport with "unexpected status code"; and it clears the session on
  `401 Unauthorized` so a retry re-establishes one.
- The HTTP server now accepts any supported `MCP-Protocol-Version` header value
  (and a request that omits the header, assumed to be `2025-03-26` for
  backwards compatibility), rejecting only unsupported versions with
  `400 Bad Request`. Previously it rejected every request whose header was not
  the single latest version, breaking older but supported clients.
- The framework adapters (`mcpkit-axum`, `mcpkit-actix`, `mcpkit-warp`,
  `mcpkit-rocket`) now accept requests that omit the `MCP-Protocol-Version`
  header, assuming `2025-03-26` for backwards compatibility per the MCP
  Streamable HTTP specification. Previously a missing header was rejected with
  `400 Bad Request`; a present-but-unsupported value is still rejected.
- The `mcpkit-axum` and `mcpkit-actix` session stores now reap expired sessions
  when a new one is created, so the store stays bounded without a background
  cleanup task; previously sessions accumulated indefinitely because the
  cleanup routines were never invoked on the default request path. Sessions are
  also reaped if created but not initialized within the initialization timeout,
  and the `initialize` request now marks its session initialized.
- The `mcpkit-warp` and `mcpkit-rocket` session stores now reap sessions idle
  past the idle timeout when a new one is created, so the store stays bounded;
  previously their cleanup routine was never invoked on the default request
  path and sessions accumulated indefinitely.
- The framework adapters (`mcpkit-axum`, `mcpkit-actix`, `mcpkit-warp`,
  `mcpkit-rocket`) now record the protocol version and client capabilities
  negotiated at `initialize` and surface them in the `Context` passed to tools,
  resources, and prompts (and echo the negotiated version in the `initialize`
  response), instead of always reporting the latest protocol version and empty
  client capabilities.

## [0.6.0] - 2026-06-18

### Added

- `RequestId::Null` variant so error responses to an unparsable request can use
  `"id": null` as required by JSON-RPC 2.0
  ([#17](https://github.com/praxiomlabs/mcpkit/issues/17)).
- `ClientBuilder::request_timeout` to configure the per-request response timeout
  (`mcpkit-client`). Defaults to 60 seconds.

### Security

- JWT/OAuth hardening ([#20](https://github.com/praxiomlabs/mcpkit/issues/20)):
  `TokenValidation::with_allowed_algorithms` lets a relying party pin the
  accepted signing algorithms so a token cannot dictate its own (RFC 8725
  §3.1); `fetch_jwks` now refuses non-`https://` URIs to prevent key
  substitution over plaintext transport; and `PkceChallenge::verify` compares
  the challenge in constant time.
- Updated `Cargo.lock` to patched versions of vulnerable transitive and direct
  dependencies flagged by Dependabot / `cargo audit`:
  `openssl` 0.10.75 → 0.10.80, `rustls-webpki` → 0.103.13, `quinn-proto` →
  0.11.14, `jsonwebtoken` → 10.3.0, `actix-http` → 3.12.1, `bytes` → 1.11.1,
  `time` → 0.3.47, `rsa` → 0.9.10, and `rand` → 0.8.6 / 0.9.3. The remaining
  advisories (`rsa` Marvin timing sidechannel and `rustls-pemfile` unmaintained)
  are dev-only/unfixed and already documented as ignores in `deny.toml`.
- Newline-framed transports now enforce the message-size limit **during** the
  read instead of after, so a peer that streams data without a newline can no
  longer exhaust memory before the cap is checked
  ([#7](https://github.com/praxiomlabs/mcpkit/issues/7)). Covers stdio, spawned
  subprocess, Unix sockets, and Windows named pipes.

### Changed

- **(Breaking)** `Completion.total` is now `Option<usize>` and the broken
  `CompletionTotal` enum was removed
  ([#17](https://github.com/praxiomlabs/mcpkit/issues/17)). Its `Approximate`
  variant was unreachable on round-trip (both variants serialized to a bare
  integer); the MCP spec models `total` as a plain count with a separate
  `hasMore` flag.
- **The server now processes requests concurrently** instead of strictly one at
  a time ([#9](https://github.com/praxiomlabs/mcpkit/issues/9)). Requests are
  interleaved on the connection task up to `RuntimeConfig::max_concurrent_requests`
  in flight (default 100); reaching the limit applies backpressure. This also
  makes `max_concurrent_requests` a live setting
  ([#21](https://github.com/praxiomlabs/mcpkit/issues/21)) — set it via
  `ServerRuntime::with_config`.
- **Client requests now time out** instead of waiting indefinitely
  ([#5](https://github.com/praxiomlabs/mcpkit/issues/5)). Each request waits at
  most `request_timeout` (default 60s) for a response and then fails with
  `TransportErrorKind::Timeout`. Clients that issue legitimately long-running
  calls should raise the timeout or use the Tasks API.
- **Extracted LLM orchestration crates to separate [llmtk](https://github.com/praxiomlabs/llmtk) project**
  - The forge orchestration layer (provider, template, memory, embedding, chain, agent, rag, eval)
    has been moved to a dedicated LLM Toolkit workspace to maintain clear separation of concerns
  - mcpkit now focuses solely on MCP protocol implementation
  - See llmtk for LLM provider abstractions, RAG pipelines, agents, and related functionality

### Fixed

- The `Tool` builder no longer panics on a malformed `input_schema`
  ([#18](https://github.com/praxiomlabs/mcpkit/issues/18)). `with_*_param`
  previously indexed a non-object `properties` (or `input_schema`) and panicked;
  it now coerces non-object values to a fresh object.
- Macro schema fidelity ([#19](https://github.com/praxiomlabs/mcpkit/issues/19)):
  tool parameter types are now resolved by their last path segment, so qualified
  paths like `std::string::String` and `core::option::Option<T>` map to the
  correct schema instead of a confusing compile error; the dead `Option`
  "nullable" code was removed (optionality is conveyed by omitting the parameter
  from `required`); and `#[mcp_server]` on a generic impl block now fails with a
  clear error rather than emitting malformed impls.
- `McpError::ResourceAccessDenied` now has a distinct JSON-RPC error code
  ([#17](https://github.com/praxiomlabs/mcpkit/issues/17)); it previously
  collided with `ResourceNotFound` at `-32002`, so clients couldn't tell the two
  apart.

- Retry middleware: jitter is now actually randomized, and timeouts are no
  longer retried by default ([#15](https://github.com/praxiomlabs/mcpkit/issues/15)).
  The previous jitter term was always zero (`attempt % 1.0`), so coordinated
  retries didn't spread out; it now uses a real RNG. `DefaultRetryPolicy` no
  longer retries `Timeout` (a timed-out send may already have been delivered, so
  retrying could duplicate a non-idempotent operation) — only connection-level
  errors are retried; supply a custom `RetryPolicy` to opt back in.
- The WebSocket `max_message_size` setting is now actually applied
  ([#13](https://github.com/praxiomlabs/mcpkit/issues/13)). Both the client and
  server build a `tungstenite::WebSocketConfig` from the configured limit and
  pass it via `connect_async_with_config` / `accept_hdr_async_with_config`;
  previously the value was dropped and tungstenite's default was always used.
- The `#[mcp(default = ..., min = ..., max = ...)]` parameter attribute is now
  functional ([#14](https://github.com/praxiomlabs/mcpkit/issues/14)). It was
  documented but a no-op — the parsed attributes were never emitted, so
  generated tool schemas omitted `default`/`minimum`/`maximum`. The macro now
  parses these (and strips the helper attribute, along with parameter doc
  comments, so the impl still compiles) and emits them into the JSON Schema.
- The default in-memory rate limiter now isolates clients per key
  ([#11](https://github.com/praxiomlabs/mcpkit/issues/11)). `InMemoryStore`
  previously used a single global bucket and ignored the key, so one noisy
  client throttled everyone. It now keeps an independent bucket per key, bounded
  by an LRU-evicted map (default 10,000 keys) to cap memory.
- `SpawnedTransport` now actually terminates the child process when dropped
  ([#12](https://github.com/praxiomlabs/mcpkit/issues/12)). The child is spawned
  with `kill_on_drop`, so dropping the transport kills it instead of leaking a
  process; the rustdoc was corrected to match (it previously promised a
  graceful-then-timeout shutdown that wasn't implemented).
- JWT `required_claims` are now actually enforced by the signature-verifying
  validator ([#10](https://github.com/praxiomlabs/mcpkit/issues/10)). Previously
  custom claims were silently ignored and configuring any `required_claims`
  dropped the default `exp`-presence requirement (a per-item
  `set_required_spec_claims` loop that only handled registered claims and
  replaced the set). Required claims are now checked on the decoded token.
- A panicking request handler no longer tears down the whole connection
  ([#9](https://github.com/praxiomlabs/mcpkit/issues/9)). Each request runs with
  panic isolation; a panic is caught and returned as a JSON-RPC internal error,
  and the server keeps serving subsequent requests.
- `Context::cancelled()` no longer busy-spins at 100% CPU while waiting
  ([#8](https://github.com/praxiomlabs/mcpkit/issues/8)). The cancellation
  future now parks on an `event_listener::Event` and is woken by `cancel()`,
  instead of re-waking itself on every poll.
- Connection pool no longer leaks `in_use` capacity
  ([#6](https://github.com/praxiomlabs/mcpkit/issues/6)). A failing connection
  factory now rolls back its reserved slot, and dropping a
  `PooledConnectionGuard` releases its slot, so the pool can no longer drain to
  permanent exhaustion. `in_use`/`peak_in_use` are tracked with atomics so a
  slot can be freed from synchronous (drop) contexts.
- In-flight client requests now fail fast with `ConnectionClosed` when the
  connection drops, instead of hanging until their timeout; pending response
  slots are reclaimed on timeout and disconnect to prevent unbounded growth
  ([#5](https://github.com/praxiomlabs/mcpkit/issues/5))
- Resolved Clippy lints surfaced by newer stable toolchains (`map_unwrap_or`,
  `unnecessary_map_or`, `unnecessary_sort_by`) across `mcpkit-core`,
  `mcpkit-transport`, `mcpkit-server`, and `mcpkit-testing`, restoring a clean
  `clippy -D warnings` on current stable Rust
- Clippy warning for `from_str` method naming in `mcpkit-core::auth::jwt` (renamed to `parse`)
- Clippy warnings in `mcpkit-transport` for single-pattern match expressions

## [0.5.0] - 2025-12-25

### Added

- **gRPC transport** with bidirectional streaming (`mcpkit-transport::grpc`)
  - Full protobuf-based MCP message transport
  - Server and client implementations using tonic
  - Automatic protobuf code generation via prost-build
- **mcpkit-rocket** web framework integration
  - Rocket 0.5 support for MCP servers
  - JSON-RPC endpoint handling
  - Session management with SSE support
- **mcpkit-warp** web framework integration
  - Warp 0.3 support for MCP servers
  - Lightweight alternative to Axum/Actix
  - CORS and session management
- **Framework-specific examples**
  - `rocket-server-example` demonstrating Rocket integration
  - `warp-server-example` demonstrating Warp integration
- **Multi-service distributed architecture example**
  - Gateway pattern with service mesh
  - Tools service and resources service separation
  - Docker Compose and Kubernetes deployment configs
- **Deployment configurations**
  - Docker multi-stage build optimized for production
  - Kubernetes manifests with health checks and resource limits
  - Docker Compose for local development

### Changed

- Updated release workflow to include mcpkit-rocket and mcpkit-warp
- Improved clippy lint configuration for generated protobuf code

### Fixed

- Clippy warnings in generated protobuf code
- Redundant closure warnings in integration tests
- Format string warnings in error formatting

## [0.4.0] - 2025-12-24

### Added

- **`#[mcp_client]` macro** for building MCP clients with handler attributes
  - `#[sampling]` for sampling/create_message handlers
  - `#[elicitation]` for user elicitation handlers
  - `#[roots]` for dynamic root listing
  - Lifecycle hooks: `#[on_connected]`, `#[on_disconnected]`
  - Notification handlers: `#[on_task_progress]`, `#[on_resource_updated]`, etc.
- **Protocol extension infrastructure** (`mcpkit-core::extension`)
  - Extension registry for MCP protocol extensions
  - App discovery and templates support
  - OAuth protected resource discovery endpoints
- **Debug tooling** for protocol inspection (`mcpkit-core::debug`)
  - Session recording and playback
  - Protocol validation utilities
- **Connection pool improvements** with lifecycle management
  - Pre-warming, health checks, and graceful shutdown
  - Configurable idle timeouts and connection limits
- **OpenTelemetry and Prometheus integration** (`mcpkit-transport::telemetry`)
  - Distributed tracing with OpenTelemetry
  - Metrics collection with Prometheus
- **Windows named pipes transport** for Windows IPC
- **Message batching middleware** for improved throughput
- **WASM support** for `wasm32-unknown-unknown` target in mcpkit-core
- **Health check support** in mcpkit-server
- **Async test helpers** in mcpkit-testing
- **Smol runtime example** demonstrating non-Tokio usage
- **Client guide documentation** (`docs/client-guide.md`)
- **1.0 release documentation** with migration guides

### Changed

- Updated prometheus dependency from 0.13 to 0.14 (fixes RUSTSEC-2024-0437)
- Improved test coverage for `#[mcp_client]` macro (18 new tests)

### Removed

- Removed unused `sqlx` dependency from database-server example
- Removed unused `actix-web-actors` dependency from mcpkit-actix

### Fixed

- Fixed `#[non_exhaustive]` on `PoolConfig` and `PoolStats` for future compatibility
- Fixed broken doc link for Windows transport

## [0.3.0] - 2025-12-23

### Added

- **Zero-copy message handling** with `bytes` crate for improved parsing performance
  - New `BufReader::read_line_bytes()` method returns `Bytes` directly
  - `StdioTransport` now uses `serde_json::from_slice` to avoid String allocations
  - Re-exported `Bytes` and `BytesMut` types from mcpkit-transport
- **Filesystem server example** (`examples/filesystem-server/`) demonstrating:
  - Sandboxed file operations with path traversal protection
  - Tools: read_file, write_file, list_directory, search_files, and more
- **Stress testing CI workflow** (`.github/workflows/stress-test.yml`)
  - Criterion benchmarks with performance regression detection
  - Long-running stability tests on schedule
- **Fuzz target** for protocol version parsing (`fuzz_protocol_version`)
- **Developer tooling improvements**:
  - `just install-tools` recipe for automated dev environment setup
  - `just install-tools-minimal` for CI-only tools
  - Updated CONTRIBUTING.md with development tools documentation
- **Integration test script** (`scripts/integration-test.sh`) for comprehensive testing
- **Performance baseline documentation** (`docs/performance-baseline.md`) with Criterion benchmark results
- **Claude Desktop WSL2 guide** (`docs/claude-desktop-wsl2.md`) with verified configuration
- **Client-example improvements**:
  - Now uses filesystem-server for real MCP protocol testing
  - Gracefully handles unsupported methods (resources, prompts)
  - Updated documentation

### Changed

- **Rate limiter optimization**: Replaced manual CAS loop with `fetch_update` for cleaner, more idiomatic code
- **Security advisory handling**: Updated `deny.toml` with documented ignores for:
  - RUSTSEC-2024-0436 (paste via rmcp - dev-dependency only)
- **async-std replaced with smol**: The `async-std-runtime` feature now maps to `smol-runtime`
  - async-std has been discontinued ([RUSTSEC-2025-0052](https://rustsec.org/advisories/RUSTSEC-2025-0052.html))
  - Existing code using `async-std-runtime` will continue to compile (maps to smol)
  - For explicit runtime choice, use `tokio-runtime` (default) or `smol-runtime`

### Removed

- **async-std dependency** removed from mcpkit-transport
  - Feature aliases preserved for backwards compatibility (`async-std` → `smol-runtime`)

### Fixed

- **Filesystem server stdout pollution**: Changed `println!` and tracing to use stderr, keeping stdout clean for JSON-RPC messages
- Various clippy warnings (`map_unwrap_or`, `items_after_statements`, `collapsible_if`)
- cfg-gated imports in websocket server to avoid unused import warnings

## [0.2.5] - 2025-12-17

### Added

- `EventStore` for SSE message resumability in mcpkit-axum and mcpkit-actix (MCP Streamable HTTP spec compliance)
- Re-export `Serialize`, `Deserialize` traits and `json!` macro from mcpkit prelude
- Client message routing integration tests

### Changed

- Removed `Clone` requirement from handler types in mcpkit-actix (API improvement)
- Improved README documentation for mcpkit-axum and mcpkit-actix crates

### Fixed

- **Critical**: Async cancellation bug in `BufReader::read_line()` causing message duplication in `SpawnedTransport`
  - Root cause: Setting `pos=0` before await point caused duplicate reads when futures were cancelled by `tokio::select!`
  - Manifested as "unknown request" warnings when using client with spawned servers
- Justfile `clippy` recipes now use `--workspace` flag to lint all workspace members
- Justfile `examples` recipe now correctly builds workspace packages instead of using `--examples` flag

## [0.2.4] - 2025-12-17

### Added

- `resources/templates/list` support for resource template discovery
- `McpRouter` struct in mcpkit-axum and mcpkit-actix for type-safe route mounting
- Exported routing functions (`route_prompts`, `route_resources`, `route_tools`) from mcpkit-server

### Changed

- Unified HTTP crate APIs: renamed `McpConfig` to `McpRouter` in mcpkit-actix for consistency with mcpkit-axum
- All MCP request methods now route through handler traits for consistent behavior
- HTTP integration ergonomics improved with builder pattern refinements

### Fixed

- Protocol version references updated from 2025-06-18 to 2025-11-25 across all crates
- Crate consistency issues preventing crates.io publishing (missing route_* exports)
- Documentation standardized across all crates

## [0.2.3] - 2025-12-17

### Added

- `From<String>` and `From<&str>` implementations for `ToolOutput` for ergonomic returns
- Expansion tests for resource-only and prompt-only servers
- Tool annotation documentation with usage examples
- Error handling guidance for `ToolOutput::error()` vs `Result<ToolOutput, McpError>`
- Transport availability documentation table with feature flags
- Stateful handler example in minimal-server demonstrating `AtomicU64` usage

### Changed

- Split `error.rs` (1200+ lines) into focused submodules: `types`, `codes`, `context`, `details`, `jsonrpc`, `transport`
- Split `http.rs` (42KB) into submodules: `client`, `server`, `sse`, `config`
- Split `websocket.rs` (36KB) into submodules: `client`, `server`, `config`
- Split `pool.rs` (36KB) into submodules: `config`, `connection`, `manager`
- Added 673+ `#[must_use]` annotations across all crates for clearer API semantics
- Server `initialize` response now uses handler's `server_info()` instead of hardcoded values

### Fixed

- Server name/version attributes from `#[mcp_server]` macro now properly appear in initialize response
- Unused import warnings for feature-gated HTTP headers
- Macro-generated code now uses facade crate paths (`::mcpkit::`) for proper resolution

## [0.2.2] - 2025-12-17

### Fixed

- Eliminated panic path in rate limiter when sliding window exceeds process uptime

### Added

- Troubleshooting guide documentation
- Release checklist for systematic release validation
- Justfile recipes for release workflow (`wip-check`, `panic-audit`, `metadata-check`)
- Code coverage CI job with Codecov integration

### Changed

- Documentation version references updated from 0.1 to 0.2
- Architecture diagram crate names corrected (`mcp-*` to `mcpkit-*`)
- MSRV reference updated in CONTRIBUTING.md (1.75 to 1.85)
- Dockerfile base image updated to `rust:1.85-bookworm`
- Codecov configuration paths updated to current crate structure
- Advisory ignore documented in deny.toml (RUSTSEC-2025-0052)

## [0.2.1] - 2025-12-13

### Added

- `ToolBuilder` annotation methods: `destructive()`, `idempotent()`, `read_only()`
- Warning log when server returns unknown protocol version (falls back to latest)
- Comprehensive test coverage for:
  - Tool annotations and metadata
  - Protocol version edge cases
  - HTTP session recovery
  - Resource template URI matching
  - Async cancellation propagation

### Fixed

- HTTP header casing changed to lowercase for HTTP/2 compatibility (`mcp-session-id`, `mcp-protocol-version`)
- Clarified TODO comment in macro crate (annotations were already implemented)

### Changed

- Updated documentation to reflect lowercase HTTP headers

## [0.2.0] - 2025-12-12

### Added

- Client APIs for Tasks (list, get, cancel)
- Client APIs for Completions (prompt arguments, resource arguments)
- Client resource subscription support (subscribe, unsubscribe)
- Client progress callback handling via `ClientHandler` trait
- Server-level request metrics (`ServerMetrics`)
- Comprehensive error scenario tests
- Middleware interaction tests
- Async cancellation tests
- Justfile for modern development workflow (73 recipes)

### Changed

- Expanded custom transport documentation with Redis example
- Enhanced security documentation with OWASP Top 10 alignment

## [0.1.0] - 2025-12-11

### Added

- Initial release of the Rust MCP SDK
- Unified `#[mcp_server]` macro for defining MCP servers
- `#[tool]` attribute for defining tools with automatic schema generation
- `#[resource]` attribute for defining resource handlers
- `#[prompt]` attribute for defining prompt handlers
- `#[derive(ToolInput)]` for generating JSON Schema from structs
- Full MCP 2025-11-25 protocol support
- Tasks capability for long-running operations
- Multiple transport implementations:
  - Standard I/O (stdio)
  - HTTP with Server-Sent Events (SSE)
  - WebSocket with auto-reconnect
  - Unix domain sockets
  - In-memory transport for testing
- Connection pooling for both transports and clients
- Middleware layer system:
  - Logging middleware
  - Timeout middleware
  - Retry middleware with exponential backoff
  - Metrics middleware
- Typestate pattern for connection lifecycle
- Rich error handling with context chains
- Comprehensive test suite
- Example servers (minimal-server, full-server, database-server)
- Client library with connection pooling
- Server discovery for stdio-based servers
- `mcpkit-testing` crate for test utilities
- Protocol version detection and capability negotiation

[Unreleased]: https://github.com/praxiomlabs/mcpkit/compare/v0.6.0...HEAD
[0.6.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.5.0...v0.6.0
[0.5.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.4.0...v0.5.0
[0.4.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.5...v0.3.0
[0.2.5]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.4...v0.2.5
[0.2.4]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.3...v0.2.4
[0.2.3]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.2...v0.2.3
[0.2.2]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.1...v0.2.2
[0.2.1]: https://github.com/praxiomlabs/mcpkit/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/praxiomlabs/mcpkit/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/praxiomlabs/mcpkit/releases/tag/v0.1.0

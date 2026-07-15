# `patinad` Stage 2A Event Stream Implementation Plan (Completed)

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Do not use subagents for this plan.

**Goal:** Give `patinad` a bounded, authenticated, reconnectable localhost event stream and an explicit capability contract before it takes ownership of tracking.

**Architecture:** Add a host-neutral runtime event hub that assigns monotonic sequence numbers, retains a small replay window, and implements the existing `RuntimeEventSink`. Expose daemon events through authenticated Server-Sent Events (SSE) on the existing bounded HTTP server, with `Last-Event-ID` replay and lag diagnostics. Add a capability endpoint so desktop and future browser/TUI clients can distinguish transport availability from tracking ownership without inferring readiness from errors.

**Tech Stack:** Rust, Tokio broadcast channels, the existing bounded localhost HTTP transport, serde/JSON, OpenAPI 3.1.

---

## File Ownership

- `src-tauri/src/engine/runtime_event.rs`: typed runtime events, sequenced envelopes, bounded replay, subscriptions, and sink implementation.
- `src-tauri/src/engine/api/http.rs`: bounded request header parsing plus authenticated SSE response framing and lifecycle.
- `src-tauri/src/engine/api/router.rs`: route selection between JSON and event-stream responses.
- `src-tauri/src/engine/api/server.rs`: event hub lifetime and connection injection.
- `src-tauri/src/engine/api/surface.rs`: exact endpoint availability per host surface.
- `src-tauri/src/engine/api/handlers/capabilities.rs`: machine-readable host and protocol capability response.
- `src-tauri/src/engine/api/handlers/openapi.rs`: event and capability endpoint/schema declarations.
- `src-tauri/src/app/daemon/mod.rs`: daemon event hub assembly without starting tracking.
- `src-tauri/src/app/daemon/runtime.rs`: shutdown ordering for API/event subscribers, SQLite, and lease.

### Task 1: Add the Bounded Runtime Event Hub

**Files:**
- Modify: `src-tauri/src/engine/runtime_event.rs`

- [x] Write failing tests proving emitted events receive strictly increasing sequence IDs.
- [x] Write failing tests proving a late subscriber receives only retained events after its requested sequence.
- [x] Write failing tests proving replay capacity is bounded and a too-old cursor requests a full state resync.
- [x] Implement `RuntimeEventEnvelope`, `RuntimeEventHub`, and `RuntimeEventSubscription` with a bounded replay queue and Tokio broadcast sender.
- [x] Implement `RuntimeEventSink` for the hub without changing existing Tauri and memory sink behavior.
- [x] Run focused runtime event tests.
- [x] Retain the task for the coherent Stage 2A closure commit.

### Task 2: Add Capability Negotiation

**Files:**
- Create: `src-tauri/src/engine/api/handlers/capabilities.rs`
- Modify: `src-tauri/src/engine/api/handlers/mod.rs`
- Modify: `src-tauri/src/engine/api/types.rs`
- Modify: `src-tauri/src/engine/api/router.rs`
- Modify: `src-tauri/src/engine/api/surface.rs`
- Modify: `src-tauri/src/engine/api/handlers/openapi.rs`

- [x] Write failing surface tests proving desktop and daemon advertise their exact capabilities and endpoint sets.
- [x] Add `/api/v1/capabilities` with protocol version, runtime host, event-stream availability, tracking ownership/readiness, browser bridge readiness, and write API availability.
- [x] Keep daemon tracking and browser bridge readiness false until their owners actually migrate.
- [x] Add field-level OpenAPI schemas for capabilities and the event envelope.
- [x] Run focused API surface, handler, and OpenAPI tests.
- [x] Retain the task for the coherent Stage 2A closure commit.

### Task 3: Expose Authenticated SSE With Replay

**Files:**
- Modify: `src-tauri/src/engine/api/http.rs`
- Modify: `src-tauri/src/engine/api/router.rs`
- Modify: `src-tauri/src/engine/api/server.rs`
- Modify: `src-tauri/src/engine/api/surface.rs`

- [x] Write failing HTTP tests proving `/api/v1/events` rejects missing or invalid bearer credentials.
- [x] Write failing HTTP tests proving a valid request receives SSE headers and a sequenced JSON event.
- [x] Write failing tests proving `Last-Event-ID` is parsed case-insensitively and replayed in order.
- [x] Add an explicit HTTP connection response enum so JSON handlers remain one-shot while event connections stream.
- [x] Send bounded replay first, then live broadcast events; emit a `resync-required` control event after replay gaps or receiver lag.
- [x] Send periodic SSE comments as keepalives and end cleanly on disconnect or server shutdown.
- [x] Run focused HTTP/router/server tests.
- [x] Retain the task for the coherent Stage 2A closure commit.

### Task 4: Assemble the Daemon Event Host

**Files:**
- Modify: `src-tauri/src/app/daemon/mod.rs`
- Modify: `src-tauri/src/app/daemon/runtime.rs`
- Modify: `src-tauri/src/engine/api/server.rs`

- [x] Write a failing daemon lifecycle test proving the event endpoint is live while the daemon owns the lease and releases the connection/listener during shutdown.
- [x] Construct one daemon-owned `RuntimeEventHub` and pass it to the API server and future runtime owners.
- [x] Preserve Stage 1 behavior: daemon still does not start tracking or write sessions.
- [x] Verify shutdown first stops event/API connections, then closes SQLite, then releases the runtime lease.
- [x] Run daemon and real TCP API tests.
- [x] Retain the task for the coherent Stage 2A closure commit.

### Task 5: Close the Stage 2A Contract

**Files:**
- Modify: `docs/working/2026-07-10-patinad-runtime-design.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap-and-prioritization.md`
- Modify: `docs/api-index.md`
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Move after completion: this plan to `docs/archive/`

- [x] Add boundary checks preventing runtime event transport from importing Tauri and preventing daemon capability claims from outrunning implemented owners.
- [x] Run formatting, full Rust tests, production clippy with warnings denied, the full frontend suite, architecture checks, and browser smoke.
- [x] Run real TCP in-process tests covering event replay, then run a temporary-XDG daemon smoke test covering capabilities, authenticated SSE handshake/keepalive, invalid auth, unchanged read-only API, and clean shutdown.
- [x] Update active docs with exact Stage 2A capability and the remaining Stage 2 owner migration.
- [x] Archive this completed plan and commit as `feat: add patinad event stream foundation`.

## Protocol Decisions

- Endpoint: authenticated `GET /api/v1/events` using `text/event-stream`.
- Authentication: existing bearer token header only; token is never accepted in the query string.
- Resume: clients send `Last-Event-ID`; replay is best effort within a bounded in-memory window.
- Recovery: replay gaps and broadcast lag produce `resync-required`; clients then reload snapshots from the read API.
- Ordering: sequence IDs are monotonic within one daemon process. A daemon restart resets the sequence, so clients must capability-check and reload snapshots after reconnect.
- Browser security: browser UI session/pairing remains Stage 3; Stage 2A does not expose the long-lived bearer token to browser storage or URLs.

## Stop Conditions

- Do not start daemon tracking, watchdog, power, audio, MPRIS, or browser bridge in Stage 2A.
- Do not make the Tauri desktop consume the daemon stream yet.
- Do not expose daemon POST endpoints or browser CORS/session changes.
- Do not retain an unbounded event history or persist events as a second activity database.
- Stop and reassess if streaming requires weakening request limits, bearer authentication, loopback binding, or shutdown guarantees.

# `patinad` Stage 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking. Do not use subagents for this plan.

**Goal:** Make desktop and `patinad` share host-neutral runtime data boundaries, expose the complete read-only local API from the daemon, and remove database access from tracking/watchdog's Tauri host dependency without moving tracking ownership yet.

**Architecture:** Add a small shared `RuntimeContext` for the SQLite pool and runtime snapshots plus a narrow `RuntimeEventSink` for tracking events. API handlers receive an `ApiRuntimeContext` assembled by each host instead of reaching through `AppHandle`; the desktop host supplies Tauri-backed runtime state, while `patinad` supplies daemon-owned state and explicit unavailable snapshots. Stage 1 keeps all API writes desktop-only and keeps tracking execution desktop-owned.

**Tech Stack:** Rust, Tokio, SQLx/SQLite, Tauri v2, existing bounded localhost HTTP transport.

---

## File Ownership

- `src-tauri/src/engine/runtime_context.rs`: host-neutral pool, clock, and shared runtime snapshot ownership.
- `src-tauri/src/engine/runtime_event.rs`: minimal event value and sink contract; Tauri and test adapters stay outside business logic.
- `src-tauri/src/engine/api/context.rs`: API-specific provider bundle built from shared runtime state.
- `src-tauri/src/engine/api/router.rs`: route dispatch only; no host state lookup.
- `src-tauri/src/engine/api/server.rs`: accepts an API context from desktop or daemon assembly.
- `src-tauri/src/engine/api/handlers/*`: business queries over explicit context/pool.
- `src-tauri/src/app/runtime.rs`: desktop adapter assembly.
- `src-tauri/src/app/daemon/mod.rs`: daemon adapter assembly.
- `src-tauri/src/engine/tracking/watchdog.rs`: shared pool and event sink consumption.
- `src-tauri/src/engine/tracking/runtime.rs`: shared context/event boundary consumption while desktop remains host.

### Task 1: Establish Shared Runtime Context

**Files:**
- Create: `src-tauri/src/engine/runtime_context.rs`
- Create: `src-tauri/src/engine/runtime_event.rs`
- Modify: `src-tauri/src/engine/mod.rs`
- Test: inline unit tests in the two new modules

- [x] Write failing tests proving a context returns its pool and current time through an injectable clock.
- [x] Write failing tests proving an in-memory event sink records typed tracking events in order.
- [x] Implement the minimal context, clock, event value, and sink contract.
- [x] Run focused Rust tests and `cargo clippy --bins -- -D warnings`.
- [x] Commit as `refactor: add shared runtime context boundaries` (combined with Task 2 because the production context is consumed there).

### Task 2: Remove `AppHandle` From Database-Only API Handlers

**Files:**
- Create: `src-tauri/src/engine/api/context.rs`
- Modify: `src-tauri/src/engine/api/mod.rs`
- Modify: `src-tauri/src/engine/api/handlers/sessions.rs`
- Modify: `src-tauri/src/engine/api/handlers/trend.rs`
- Modify: `src-tauri/src/engine/api/handlers/web_activity.rs`
- Modify: `src-tauri/src/engine/api/handlers/apps.rs`
- Modify: `src-tauri/src/engine/api/handlers/settings.rs`
- Test: handler unit tests using a temporary prepared SQLite pool

- [x] Write failing tests that call representative sessions, summary, trend, web activity, apps, and tracker-settings reads without a Tauri application.
- [x] Add `ApiRuntimeContext` with explicit pool access.
- [x] Convert database-only read handlers to accept `&ApiRuntimeContext` or `&Pool<Sqlite>`.
- [x] Keep app/settings POST handlers desktop-only but make their database dependency explicit.
- [x] Run focused handler tests and clippy.
- [x] Commit as `refactor: make API database handlers host neutral`.

### Task 3: Add Host-Neutral Runtime Snapshot Providers

**Files:**
- Modify: `src-tauri/src/engine/api/context.rs`
- Modify: `src-tauri/src/engine/api/handlers/health.rs`
- Modify: `src-tauri/src/engine/api/handlers/diagnostics.rs`
- Modify: `src-tauri/src/engine/api/handlers/tools.rs`
- Modify: `src-tauri/src/engine/api/handlers/ai.rs`
- Modify: `src-tauri/src/engine/tools/mod.rs`
- Modify: `src-tauri/src/engine/web_activity/mod.rs`
- Test: provider and aggregate-response unit tests

- [ ] Write failing tests for current-window unavailable/ready states and daemon diagnostics degradation.
- [ ] Expose read-only snapshots from their state owners without `AppHandle`.
- [ ] Build desktop and daemon provider sets with explicit capability availability.
- [ ] Convert current, diagnostics, tools, and AI aggregate handlers to the shared API context.
- [ ] Verify missing live runtime state produces contract-valid degraded data instead of panics or false readiness.
- [ ] Commit as `refactor: share API runtime snapshot providers`.

### Task 4: Unify Desktop and Daemon Read-Only Routing

**Files:**
- Modify: `src-tauri/src/engine/api/surface.rs`
- Modify: `src-tauri/src/engine/api/router.rs`
- Modify: `src-tauri/src/engine/api/server.rs`
- Modify: `src-tauri/src/engine/api/handlers/openapi.rs`
- Modify: `src-tauri/src/engine/api/configuration.rs`
- Modify: `src-tauri/src/app/runtime.rs`
- Modify: `src-tauri/src/app/daemon/mod.rs`
- Modify: `src-tauri/src/app/daemon/runtime.rs`
- Test: route registry, server lifecycle, and real HTTP tests

- [ ] Define a daemon read-only surface containing every desktop GET endpoint and no POST endpoint.
- [ ] Write a bidirectional OpenAPI/route registry test for that surface.
- [ ] Make the shared router receive `Arc<ApiRuntimeContext>` and `ApiSurface`.
- [ ] Assemble the same handler path from desktop and daemon hosts.
- [ ] Run a real temporary-XDG daemon smoke test for health, sessions, summary, settings, apps, and a rejected POST.
- [ ] Commit as `feat: expose complete read-only API from patinad`.

### Task 5: Decouple Tracking and Watchdog Data/Event Access

**Files:**
- Modify: `src-tauri/src/engine/tracking/runtime.rs`
- Modify: `src-tauri/src/engine/tracking/runtime/support.rs`
- Modify: `src-tauri/src/engine/tracking/watchdog.rs`
- Modify: `src-tauri/src/app/runtime_tasks.rs`
- Modify: `src-tauri/src/app/runtime.rs`
- Test: tracking event and watchdog stale-session tests

- [ ] Write failing tests that run watchdog sealing against a prepared pool and memory event sink without Tauri.
- [ ] Extract the watchdog loop body into a context-driven iteration.
- [ ] Route tracking data-change emission through `RuntimeEventSink`.
- [ ] Keep foreground polling and Tauri lifecycle assembly in the desktop host for Stage 1.
- [ ] Verify session sealing and event ordering remain unchanged.
- [ ] Commit as `refactor: detach tracking data access from Tauri host`.

### Task 6: Stage 1 Contract and Documentation Closure

**Files:**
- Modify: `docs/working/2026-07-10-patinad-runtime-design.md`
- Modify: `docs/architecture.md`
- Modify: `docs/roadmap-and-prioritization.md`
- Modify: `docs/api-index.md`
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Move after completion: this plan to `docs/archive/`
- Test: architecture and full repository validation

- [ ] Add architecture guard tests preventing API handlers and watchdog data logic from importing `tauri::AppHandle`.
- [ ] Run `cargo fmt`, full Rust tests, clippy with warnings denied, `npm test`, `npm run test:replay`, and `npm run build`.
- [ ] Perform a desktop regression smoke test and daemon temporary-XDG read-only API smoke test.
- [ ] Update active docs to state the exact Stage 1 capability and remaining Stage 2 ownership.
- [ ] Archive this completed plan and commit as `test: verify patinad stage one boundaries`.

## Stop Conditions

- Do not start daemon tracking, watchdog, power, audio, MPRIS, or browser bridge in this stage.
- Do not expose POST endpoints from `patinad`.
- Do not create a second query implementation for daemon routes.
- Stop and reassess if a provider requires arbitrary Tauri state access or if `RuntimeContext` begins owning desktop UI concerns.

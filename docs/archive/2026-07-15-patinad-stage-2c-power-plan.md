# `patinad` Stage 2C Power Lifecycle Plan

> Status: completed and verified on 2026-07-15.

**Goal:** Let the Stage 2B tracking preview observe Linux lock, unlock, suspend, resume, and shutdown directly from systemd-logind without depending on Tauri.

**Architecture:** `platform/linux/power.rs` owns D-Bus discovery and typed lifecycle events. Desktop and daemon hosts consume the same event source through separate thin adapters. The tracking engine remains the only owner of session sealing decisions.

## Task 1: Complete lifecycle semantics

- Add shutdown to the lifecycle states that seal active sessions.
- Preserve idempotence across lock, suspend, shutdown, and normal runtime shutdown.
- Add frontend refresh policy coverage for `session-ended-shutdown`.

## Task 2: Extract a host-neutral Linux event source

- Remove `AppHandle`, global `OnceLock`, and tracking engine calls from the Linux platform watcher.
- Resolve the current logind session through `GetSessionByPID`, with `XDG_SESSION_ID` as an explicit first choice when present.
- Subscribe to Manager `PrepareForSleep` / `PrepareForShutdown` and Session `Lock` / `Unlock`.
- Emit typed `ready`, `lock`, `unlock`, `suspend`, `resume`, and `shutdown` events through a bounded Tokio channel.
- Support explicit cancellation and fail visibly if a D-Bus stream ends.

## Task 3: Preserve the desktop adapter

- Keep `power::start(AppHandle)` as a thin desktop assembly function.
- Continue emitting the existing Tauri event names and payloads.
- Route lifecycle mutations through the existing host-neutral tracking handler.
- Do not change desktop startup ordering or default ownership.

## Task 4: Add the daemon power task

- Start power ownership only together with explicit daemon `--track` mode.
- Reuse the daemon runtime context and event hub.
- Retry failed D-Bus subscriptions with bounded exponential backoff.
- Stop and await the power task before stopping tracking, closing SQLite, or releasing the lease.

## Task 5: Verify and close Stage 2C

- Unit-test signal mapping, shutdown sealing, duplicate lifecycle idempotence, and task cancellation.
- Run the complete Rust and frontend minimum validation bars.
- Start an isolated Dev daemon and verify logind readiness, live tracking API, and clean exit without forcing a real suspend or lock.
- Update active architecture, roadmap, README, API notes, and patinad runtime design.
- Archive this plan and commit the coherent Stage 2C change on `feature/patinad-daemon` without pushing.

## Stop Conditions

- Do not force the workstation to lock, suspend, or shut down during automated validation.
- Do not migrate audio, MPRIS, browser activity, systemd service installation, or desktop client transport.
- Do not make daemon tracking the default.
- Do not access Production profile data or installed package files.

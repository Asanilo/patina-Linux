# `patinad` Stage 2B Tracking Owner Plan

> Status: completed and verified on 2026-07-15.

**Goal:** Let `patinad` explicitly own tracking and watchdog execution with `--track`, while preserving the current read-only daemon mode and the embedded desktop tracker until the desktop becomes a daemon client.

**Architecture:** Keep one tracking engine implementation. Extract the remaining Tauri host adapters from the loop, give each host an owned runtime snapshot state and event sink, and add cancellation-aware tracking/watchdog tasks to the daemon lifecycle. The profile lease remains the single-writer guard.

## Task 1: Define the migration mode and truthful capabilities

- Add `track: bool` to `DaemonRunOptions` and parse `--track`.
- Reject `--track` without `--serve-api` so the experimental mode always has an observable control and diagnostics surface.
- Let daemon API runtime context expose its owned tracking snapshot only in tracking mode.
- Report daemon tracking `owned=true` in tracking mode and `ready=true` only after a snapshot exists.
- Preserve current daemon capability degradation without `--track`.

## Task 2: Remove the Tauri host from the tracking loop

- Make the shared tracking runner accept `RuntimeContext`, `TrackingRuntimeSnapshotState`, `RuntimeEventSink`, and shutdown signal.
- Keep the existing desktop `run(AppHandle, ...)` as a thin adapter.
- Route active-window and tracking-data notifications through the shared event boundary without changing existing desktop event names or payloads.
- Keep polling, transition, continuity, settings, and startup self-heal in their existing owners.

## Task 3: Add cancellable daemon tracking and watchdog ownership

- Assemble daemon-owned health state, snapshot state, event hub, and shared runtime context.
- Start tracking and watchdog only for `--track`.
- Add bounded restart behavior for unexpected task failure without spawning duplicate owners.
- On shutdown, signal background tasks first, await them, stop the event stream/API, close SQLite, and finally release the runtime lease.

## Task 4: Verify the daemon owner boundary

- Unit-test option parsing and invalid combinations.
- Test daemon API current-state and capability behavior with an injected snapshot.
- Test background task shutdown before SQLite and lease release.
- Run isolated temporary-XDG daemon smoke tests with an ephemeral API port.
- Verify the default daemon mode does not write sessions.

## Task 5: Close Stage 2B

- Update architecture, roadmap, active patinad design, README status, and API documentation.
- Record that power, audio, MPRIS, browser activity bridge, and desktop client migration remain later work.
- Run Rust formatting/tests, architecture checks, and the required frontend validation bar.
- Move this completed plan to `docs/archive/` and commit the coherent Stage 2B change on `feature/patinad-daemon`.

## Stop Conditions

- Do not make `--track` the daemon default in this stage.
- Do not disable the embedded desktop tracker yet.
- Do not run desktop and daemon tracking against the same profile.
- Do not migrate power, audio, MPRIS, browser bridge, systemd, or browser UI in this change.
- Do not touch installed package files, production data, autostart entries, or the Production profile during validation.

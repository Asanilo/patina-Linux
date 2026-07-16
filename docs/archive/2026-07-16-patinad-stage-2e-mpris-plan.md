# `patinad` Stage 2E MPRIS Plan (Completed)

## Goal

Move Linux MPRIS participation ownership into explicit daemon tracking preview while preserving correct player-to-window matching and keeping media failure independent from ordinary tracking.

## Completed Scope

- Replaced the Linux tracking loop's hidden global MPRIS lookup with an injected `MediaSignalSource` handle.
- Kept a thin global adapter for the embedded Tauri desktop runtime during migration.
- Added daemon-owned startup, cancellation, and shutdown ordering.
- Removed Tauri async runtime use from the host-neutral MPRIS query path.
- Kept a bounded snapshot of available players instead of only the first active player.
- Preferred the player matching the current foreground window, including explicit paused state.
- Verified unit tests, a live session D-Bus query, daemon lifecycle, frontend, replay, build, and architecture checks.

## Deferred

- Browser activity bridge ownership
- Default owner switch from embedded desktop tracking to `patinad`
- systemd user service and release packaging

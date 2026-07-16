# `patinad` Stage 2F Browser Bridge Plan (Completed)

## Goal

Move the authenticated local browser activity bridge into explicit daemon tracking preview without changing the Chromium or Firefox/Zen extension protocol.

## Completed Scope

- Extracted browser token validation, privacy policy, foreground-browser checks, SQLite writes, and runtime event emission from Tauri `AppHandle`.
- Replaced detached Tauri listener tasks with one loopback-only transport shared by desktop and daemon hosts.
- Bounded request headers, bodies, request duration, client task lifetime, and shutdown wait time.
- Added explicit listener readiness to diagnostics and capability negotiation.
- Added daemon startup repair, tracking-event boundary sealing, and ordered shutdown sealing.
- Kept default daemon mode read-only and kept Tauri desktop as the released default tracking owner.
- Verified invalid-token rejection, Zen foreground recording, stalled-client shutdown, listener release, and daemon capability readiness.

## Deferred

- Runtime browser bridge setting writes while daemon owns the profile
- Default owner switch from embedded desktop tracking to `patinad`
- Browser UI, desktop daemon client, and systemd user service

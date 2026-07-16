# `patinad` Stage 2D Audio Plan (Completed)

## Goal

Move Linux audio participation ownership into explicit daemon tracking preview without duplicating tracking logic or making audio availability a prerequisite for ordinary window tracking.

## Completed Scope

- Replaced the Linux tracking loop's hidden global audio lookup with an injected `AudioSignalSource` handle.
- Kept a thin global adapter for the embedded Tauri desktop runtime during migration.
- Added daemon-owned startup, persisted enablement, cancellation, and shutdown ordering.
- Preserved the distinction between no audio, stale data, timeout, and probe failure.
- Filtered corked PulseAudio streams and widened the outer query deadline so it can contain both connection and enumeration phases.
- Verified unit, live pipewire-pulse, daemon lifecycle, frontend, replay, build, and architecture checks.

## Deferred

- MPRIS ownership
- Browser activity bridge ownership
- Default owner switch from embedded desktop tracking to `patinad`
- systemd user service and release packaging

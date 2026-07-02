# Linux Storage Migration And WebView Cache Design

**Status:** Approved design

**Date:** 2026-07-02

**Scope:** Batch B, sub-project 1

## Goal

Give Linux users explicit control over Patina's primary data directory and WebView data directory without weakening database integrity, creating silent fallback databases, or introducing a general-purpose recursive deletion surface.

## Product Boundaries

- Patina remains local-first and Linux-first, with GNOME Wayland as the primary desktop environment.
- Custom storage is intended for stable local mounts. Removable and network mounts are not guaranteed.
- An unavailable custom data directory is a startup error. Patina must not silently fall back to the default database or create a new empty database.
- The first release does not delete retired data directories. Users retain the old copy for manual inspection and rollback.
- The only recursive deletion feature in this scope is deletion of the exact, regenerable Linux WebKitGTK cache directory before a WebView starts.
- API credentials remain in the stable default product data directory and are not moved with the activity database.

## Linux Path Model

Patina keeps three distinct path roles:

| Role | Default | Contents |
| --- | --- | --- |
| Storage control | `${XDG_CONFIG_HOME:-~/.config}/Patina/` | Anchors, pending migration, maintenance state |
| Product data | `${XDG_DATA_HOME:-~/.local/share}/Patina/` | `patina.db`, backups, API token, current legacy WebView profile |
| WebView profile | `${XDG_DATA_HOME:-~/.local/share}/Patina/` for compatibility | WebKitGTK persistent state and `WebKitCache/` |

The current default WebView root remains unchanged for upgrade compatibility. Tauri's `data_directory` contains persistent `localstorage/`, not only disposable cache, so it must not be moved wholesale to `XDG_CACHE_HOME`.

When users choose custom locations:

- The data target is a Patina-owned product directory under the selected location, unless the selected directory is already the current profile's product directory.
- The WebView target is independently configurable and represents the whole WebView profile, not only `WebKitCache/`.
- New custom data and WebView roots must not be the same directory. A WebView root may be a dedicated `webview/` child of the custom data root.
- Selected paths are normalized to absolute canonical parent paths before they are persisted.
- Production, local, and development profiles use distinct product-directory names under every selected parent.

## Storage Metadata

The stable configuration directory contains small versioned JSON files:

- `data-anchor.json`: active custom data root.
- `webview-anchor.json`: active custom WebView root.
- `storage-migration-pending.json`: one restart-time migration plan.
- `storage-maintenance-state.json`: last result, error, pending cache clear, and retained source information.

Each file includes a format identifier and app profile. Production, local, and development profiles must not consume each other's anchors.

Metadata writes use a temporary file in the same directory, file sync, mode `0600`, atomic rename, and parent-directory sync. The containing configuration directory uses mode `0700` on Linux.

## Ownership And Modules

Rust owns path resolution, filesystem mutation, migration, validation, and startup failure behavior.

- `platform/app_paths.rs`: default XDG-aware profile paths only.
- `platform/storage_anchor.rs`: typed metadata reads and atomic writes.
- `platform/storage_paths.rs`: resolves defaults plus active anchors into one `StoragePaths` value.
- `platform/storage_usage.rs`: read-only size and free-space inspection.
- `platform/webview_cache.rs`: Linux WebKitGTK cache snapshot and exact allowlisted deletion.
- `data/storage_migration.rs`: preview, schedule, restart-time execution, validation, promotion, and rollback.
- `domain/storage.rs`: stable request and response types.
- `commands/storage.rs`: thin Tauri commands.

The frontend owns presentation and interaction orchestration.

- `platform/storage/storageRuntimeGateway.ts`: typed command boundary.
- `features/settings/services/storagePathDisplay.ts`: path and size display helpers.
- `features/settings/hooks/useStorageSettingsState.ts`: snapshot and action state.
- `features/settings/components/SettingsStoragePanel.tsx`: Quiet Pro storage controls embedded in Data Safety.

Storage logic must not be added to `lib.rs`, thickened into Tauri command handlers, or mixed into generic settings persistence.

## Data Migration Flow

### Preview

Preview is read-only. It resolves the target, validates path relationships, checks that an existing target is safe, estimates copied bytes, and reports required free space. Preview must not create the final target directory.

The target is rejected when it:

- Is relative or cannot be normalized.
- Is the current active root.
- Is equal to, above, or inside a conflicting managed root.
- Is inside the configuration anchor directory.
- Is a symlink, or resolves through an unsafe managed-path collision.
- Already contains `patina.db`, except during the explicit restore-default flow.
- Has no writable existing parent.
- Lacks the required payload size plus a safety margin.

### Schedule

Scheduling performs the live-database preparation that must finish before restart:

1. Export a normal Patina backup archive into the current backup directory.
2. Checkpoint the SQLite WAL.
3. Persist one pending migration document atomically.
4. Return a preview and pending state to the UI.
5. Offer `Restart now` and `Later`; migration remains cancelable until restart.

Scheduling a data move and WebView move before restart merges both target choices into the same pending operation instead of replacing one with the other.

### Restart-Time Execution

The pending migration runs in Tauri setup before SQLite pools, API handlers, tracking runtime, main WebView, or widget WebView start.

For primary data:

1. Revalidate the source, target, available space, and pending metadata.
2. Require the source `patina.db` to exist.
3. Create a sibling staging directory with a random migration identifier and an ownership marker.
4. Copy `patina.db`, SQLite sidecars if present, and `backups/`. Do not copy `remote-backup-temp/`.
5. Open the staged database without `create_if_missing`.
6. Run `PRAGMA integrity_check`, current schema preparation, and critical-table row-count comparison.
7. Promote the staged, Patina-owned data payload within the target filesystem.
8. Write the active data anchor only after successful promotion.
9. Keep the source payload unchanged and record it as the retained previous location.

For a new empty custom target, the owned staging directory can be renamed into place. The default product root is different: it may also contain the API token and the legacy WebView profile. Restore-default therefore promotes only the owned database files and `backups/`, never replaces the whole default directory. An existing default database payload is moved into a migration-specific quarantine location before promotion. It is never overwritten or deleted. If promotion or anchor update fails, the quarantined payload is restored.

For the WebView profile:

1. Classify top-level entries by owner, then copy persistent WebKitGTK state while skipping `WebKitCache/`, Patina database files, backups, temporary backup files, API credentials, and storage-control metadata.
2. Validate that the copied profile remains inside the target root.
3. Switch the WebView anchor only after the copy succeeds.
4. Keep the previous profile directory unchanged.

WebView migration never follows symlinks. For restore-default, existing WebView-owned entries are quarantined before replacement because the default root can contain both product data and WebView state. Unknown entries are preserved and reported rather than overwritten.

On success the pending document is removed and maintenance state records the result. On failure the active anchors stay unchanged, staging owned by the failed operation may be removed, the source remains intact, and a concrete maintenance error is recorded. A failed pending operation is not retried forever on every startup.

## Startup And Recovery Semantics

Path resolution distinguishes four cases:

- No anchor: use the existing default path.
- Valid anchor and available source: use the custom path.
- Invalid anchor metadata: fail startup with a specific metadata error.
- Valid anchor but missing/unreadable database: fail startup with the exact path and do not create a database.

A native startup error dialog explains that the configured directory is unavailable and that the mount must be restored before restarting. The same error is written to stderr and maintenance state when possible.

This scope does not add automatic fallback, mount polling, database merging, or a hidden anchor-reset mode.

## Deletion Safety

Patina does not expose arbitrary recursive path deletion.

Cache deletion follows all of these rules:

- The only initial Linux allowlist entry is `<active-webview-root>/WebKitCache`.
- Deletion occurs only during startup before any Patina WebView exists.
- The active WebView root and candidate are canonicalized immediately before deletion.
- The root and candidate themselves must not be symlinks.
- The candidate must be the exact expected child and remain inside the active root.
- Directory traversal skips symlinks and never follows them.
- Missing cache directories are successful no-ops.
- `localstorage/`, `storage/`, `CacheStorage/`, `mediakeys/`, HSTS state, API credentials, databases, and backups are never cache entries.

Generated staging cleanup additionally requires a matching ownership marker containing the migration identifier and format. Unknown hidden directories are left untouched.

Migration targets created by Patina use mode `0700`. Migrated database files, sidecars, backups, and generated metadata are restricted to the current user; migration must not make source permissions more permissive.

The first release provides no button to delete retained previous data. Settings may open the previous location so the user can inspect it manually.

## Settings Experience

The existing Data Safety page gains one restrained `Local storage` subpanel using existing Quiet Pro controls.

It shows:

- Current data directory, database path, backup path, and total managed size.
- Current WebView data directory, total profile size, and reclaimable cache size.
- `Open`, `Move`, and `Restore default` actions for each root where applicable.
- `Clear on restart` for WebKit cache.
- A pending migration summary with `Cancel`.
- Last migration success or actionable failure text.

Migration confirmation shows source, target, estimated bytes, required restart, and that the old copy will be retained. After scheduling, the user chooses `Restart now` or `Later`.

The panel does not present installation paths as movable data and does not call the whole WebView profile disposable cache.

## Validation Strategy

Rust unit and integration coverage must include:

- Profile-specific XDG path resolution.
- Anchor format/profile validation and `0600` atomic writes.
- Path collision, relative path, target-exists, and insufficient-space rejection.
- Pending-operation merge and cancellation.
- Successful SQLite migration with integrity and row-count validation.
- Corrupt database, missing source, failed promotion, and failed anchor-write rollback.
- Restore-default quarantine and rollback.
- WebKit persistent-state copy without `WebKitCache`.
- Exact cache deletion, missing-cache no-op, symlink rejection, and traversal escape rejection.
- Custom root unavailable without fallback database creation.

Frontend coverage must include:

- Storage snapshot formatting and unavailable states.
- Preview, confirmation, schedule, cancellation, restart-now, and restore-default interactions.
- Busy and error states that disable conflicting actions.
- UI smoke coverage for default, custom, pending, success, and failure states.

Repository validation for the completed implementation is the standard release bar: `npm test`, `npm run test:replay`, `npm run build`, focused Rust tests, and finally `npm run release:check` before release work.

## Explicit Differences From Windows 1.8.1

- Linux uses WebKitGTK `WebKitCache/`, not Windows `EBWebView/` paths.
- Linux anchors live under XDG config instead of the movable/default data root.
- The old data source is retained; it is not automatically deleted after migration.
- Cache clearing preserves Linux WebKitGTK persistent state.
- Existing targets are not destructively replaced.
- Missing custom storage blocks startup instead of allowing an empty fallback database.

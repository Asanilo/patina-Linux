# Linux Storage Migration Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add fail-closed custom data and WebView profile directories on Linux, with restart-time verified migration and narrowly allowlisted WebKit cache cleanup.

**Architecture:** Rust owns all filesystem state, validation, migration, rollback, and startup integration through focused `platform`, `data`, `domain`, and thin `commands` modules. React only consumes typed snapshots and orchestrates Settings interactions. A versioned XDG config anchor remains outside movable data roots, and no general recursive-delete command is exposed.

**Tech Stack:** Rust, Tauri 2, sqlx/SQLite, serde JSON, getrandom, fs2, React 19, TypeScript, existing Quiet Pro components, Node test runner.

**Design:** `docs/superpowers/specs/2026-07-02-linux-storage-migration-design.md`

---

## File Map

### Rust files to create

- `src-tauri/src/domain/storage.rs`: stable storage snapshots, migration previews, path kinds, and maintenance DTOs.
- `src-tauri/src/platform/storage_anchor.rs`: versioned anchor/pending/maintenance JSON with atomic user-only writes.
- `src-tauri/src/platform/storage_paths.rs`: default and anchored path resolution with fail-closed database semantics.
- `src-tauri/src/platform/storage_usage.rs`: managed-size and free-space inspection.
- `src-tauri/src/platform/webview_cache.rs`: Linux WebKitGTK cache snapshot, persistent-profile copy classification, and exact cache deletion.
- `src-tauri/src/data/storage_migration.rs`: preview, scheduling, startup execution, database validation, promotion, quarantine, and rollback.
- `src-tauri/src/commands/storage.rs`: thin Tauri command boundary.

### Rust files to modify

- `src-tauri/Cargo.toml`: add direct `fs2` dependency for free-space checks.
- `src-tauri/src/domain/mod.rs`, `src-tauri/src/platform/mod.rs`, `src-tauri/src/data/mod.rs`, `src-tauri/src/commands/mod.rs`: declare owned modules.
- `src-tauri/src/platform/app_paths.rs`: add XDG config root and stable control/data/profile defaults.
- `src-tauri/src/data/sqlite_pool.rs`: consume resolved database path and expose migration validation helpers.
- `src-tauri/src/data/backup.rs`: resolve backup directory through `StoragePaths`.
- `src-tauri/src/data/remote_backup.rs`: resolve temporary directory through `StoragePaths`.
- `src-tauri/src/app/main_window.rs`, `src-tauri/src/app/widget.rs`: use resolved WebView root and trim scheduled cache before creating a WebView.
- `src-tauri/src/app/bootstrap.rs`: register commands and execute pending migration before SQLite/runtime setup.
- `scripts/check-rust-boundaries.ts`: prevent persistent owners from bypassing `StoragePaths` after migration.

### Frontend files to create

- `src/platform/storage/storageRuntimeGateway.ts`: typed Tauri storage commands.
- `src/features/settings/services/storagePathDisplay.ts`: byte/path/status formatting.
- `src/features/settings/services/storageSettingsActions.ts`: dependency-injected preview/schedule/cancel/cache flows.
- `src/features/settings/hooks/useStorageSettingsState.ts`: isolated storage UI state and dialog orchestration.
- `src/features/settings/components/SettingsStoragePanel.tsx`: Quiet Pro local storage section.
- `tests/storageSettings.test.ts`: service and state-independent behavior tests.

### Frontend files to modify

- `src/features/settings/components/SettingsDataSafetyPanel.tsx`: compose the storage panel without growing storage logic locally.
- `src/features/settings/components/Settings.tsx`: pass the isolated storage state into Data Safety.
- `src/features/settings/hooks/useSettingsPageState.ts`: compose `useStorageSettingsState` and expose it.
- `src/features/settings/types.ts`: storage view types only where shared by Settings components.
- `src/shared/copy/uiText.ts`: Chinese and English storage copy.
- `src/App.css` or the existing Settings feature stylesheet: only semantic/token-backed layout rules if utility classes are insufficient.
- `package.json`: add focused `test:storage` and include it in `check:frontend`.
- `tests/uiSmoke.test.ts`, `tests/uiBrowserSmoke.test.ts`: storage panel default, pending, and error smoke states.

### Documentation to modify after implementation

- `docs/product-principles-and-scope.md`: state custom local storage and safe cache control as implemented local-data control.
- `docs/roadmap-and-prioritization.md`: mark the Batch B storage gap closed without adding a temporary backlog.
- `docs/linux-development-setup.md`: document default XDG paths and recovery behavior.
- `docs/linux-port-and-api-design.md`: update the implementation status only; storage remains outside the HTTP API.

---

### Task 1: Storage Domain And XDG Path Contract

**Files:**
- Create: `src-tauri/src/domain/storage.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src-tauri/src/platform/app_paths.rs`

- [ ] **Step 1: Write failing path/profile tests**

Add pure tests that exercise path derivation without a live Tauri app:

```rust
#[test]
fn linux_profile_paths_keep_control_outside_movable_data() {
    let roots = AppPathRoots {
        config: PathBuf::from("/home/u/.config"),
        data: PathBuf::from("/home/u/.local/share"),
        local_data: PathBuf::from("/home/u/.local/share"),
    };
    let paths = profile_paths(&roots, AppProfile::Production);
    assert_eq!(paths.control_root, PathBuf::from("/home/u/.config/Patina"));
    assert_eq!(paths.data_root, PathBuf::from("/home/u/.local/share/Patina"));
    assert_eq!(paths.webview_root, PathBuf::from("/home/u/.local/share/Patina"));
}

#[test]
fn custom_parent_derives_profile_owned_directory() {
    assert_eq!(
        derive_product_root(Path::new("/mnt/work"), AppProfile::Dev),
        PathBuf::from("/mnt/work/Patina Dev"),
    );
}
```

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml platform::app_paths::tests -- --nocapture`

Expected: FAIL because `AppPathRoots`, `profile_paths`, and `derive_product_root` do not exist.

- [ ] **Step 3: Implement the minimal path and DTO contract**

Add `AppProfile::key()`, pure path derivation helpers, and a `product_config_dir(app)` wrapper. Define camelCase-serialized DTOs in `domain/storage.rs`, including:

```rust
pub struct StorageSnapshot {
    pub paths: StoragePathSnapshot,
    pub sizes: StorageSizeSnapshot,
    pub webview_cache: WebviewCacheSnapshot,
    pub maintenance: StorageMaintenanceSnapshot,
    pub pending_migration: Option<StoragePendingMigrationSnapshot>,
}

pub enum StorageTargetKind { Data, Webview }
```

Do not add filesystem mutation to `app_paths.rs`.

- [ ] **Step 4: Run focused tests and format**

Run: `cargo test --manifest-path src-tauri/Cargo.toml platform::app_paths::tests -- --nocapture`

Expected: PASS.

Run: `cargo fmt --manifest-path src-tauri/Cargo.toml -- --check`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/domain/storage.rs src-tauri/src/domain/mod.rs src-tauri/src/platform/app_paths.rs
git commit -m "feat: define Linux storage path contract"
```

### Task 2: Atomic Storage Anchors

**Files:**
- Create: `src-tauri/src/platform/storage_anchor.rs`
- Modify: `src-tauri/src/platform/mod.rs`

- [ ] **Step 1: Write failing anchor tests**

Cover format/profile validation, optional reads, atomic replacement, file modes, pending operations, and maintenance state:

```rust
#[test]
fn atomic_anchor_write_restricts_permissions() {
    let root = temp_dir("anchor-mode");
    write_data_anchor_to_dir(&root, "production", PathBuf::from("/mnt/patina")).unwrap();
    let mode = fs::metadata(data_anchor_path(&root)).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn mismatched_profile_anchor_is_ignored() {
    // Write a local-profile anchor and read it as production.
    assert!(read_data_anchor_from_dir(&root, "production").unwrap().is_none());
}
```

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_anchor::tests -- --nocapture`

Expected: FAIL because the module does not exist.

- [ ] **Step 3: Implement atomic metadata persistence**

Use typed structs with fixed formats:

```rust
pub const DATA_ANCHOR_FORMAT: &str = "patina.data-anchor.v1";
pub const WEBVIEW_ANCHOR_FORMAT: &str = "patina.webview-anchor.v1";
pub const STORAGE_MIGRATION_PENDING_FORMAT: &str = "patina.storage-migration-pending.v1";
pub const STORAGE_MAINTENANCE_FORMAT: &str = "patina.storage-maintenance.v1";
```

Write to a same-directory random `.tmp` file using `getrandom`, set `0600`, call `sync_all`, rename, then sync the parent directory. Only remove exact known metadata files; never recursively delete the control directory.

- [ ] **Step 4: Run anchor tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_anchor::tests -- --nocapture`

Expected: PASS, including Linux permission assertions.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/platform/storage_anchor.rs src-tauri/src/platform/mod.rs
git commit -m "feat: persist storage anchors atomically"
```

### Task 3: Fail-Closed Storage Path Resolution

**Files:**
- Create: `src-tauri/src/platform/storage_paths.rs`
- Modify: `src-tauri/src/platform/mod.rs`

- [ ] **Step 1: Write failing resolver tests**

Use a pure `resolve_storage_paths_from(...)` seam so tests do not need a Tauri `AppHandle`:

```rust
#[test]
fn missing_custom_database_is_an_error_without_default_fallback() {
    let result = resolve_storage_paths_from(defaults, Some(custom_anchor), None);
    assert!(result.unwrap_err().contains("custom data directory"));
    assert!(!defaults.db_path.exists());
}

#[test]
fn default_path_may_start_without_an_existing_database() {
    let paths = resolve_storage_paths_from(defaults.clone(), None, None).unwrap();
    assert_eq!(paths.db_path, defaults.db_path);
}
```

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_paths::tests -- --nocapture`

Expected: FAIL because resolver APIs do not exist.

- [ ] **Step 3: Implement `StoragePaths` and strict resolution**

The resolver must:

- Permit database creation only when no custom data anchor is active.
- Return exact errors for invalid metadata, unreadable roots, and missing custom `patina.db`.
- Keep API token resolution on the stable default product data root.
- Derive `backup_dir` and `remote_backup_temp_dir` from active data root.
- Mark custom data and WebView roots independently.

- [ ] **Step 4: Run resolver tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_paths::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/platform/storage_paths.rs src-tauri/src/platform/mod.rs
git commit -m "feat: resolve custom storage without fallback"
```

### Task 4: Safe Usage Inspection And WebKit Cache Control

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/platform/storage_usage.rs`
- Create: `src-tauri/src/platform/webview_cache.rs`
- Modify: `src-tauri/src/platform/mod.rs`

- [ ] **Step 1: Write failing storage usage and deletion tests**

Cover byte counts, available-space results, exact allowlist behavior, persistent-state preservation, symlink rejection, and ownership-marker cleanup:

```rust
#[test]
fn cache_clear_only_removes_webkit_cache() {
    write_file(&root.join("WebKitCache/Version 17/Blobs/a"), 10);
    write_file(&root.join("localstorage/tauri.localstorage"), 20);
    clear_linux_webkit_cache(&root).unwrap();
    assert!(!root.join("WebKitCache").exists());
    assert!(root.join("localstorage/tauri.localstorage").exists());
}

#[cfg(unix)]
#[test]
fn cache_clear_refuses_symlink_candidate() {
    symlink(&outside, root.join("WebKitCache")).unwrap();
    assert!(clear_linux_webkit_cache(&root).is_err());
    assert!(outside.join("keep").exists());
}
```

- [ ] **Step 2: Verify the tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_usage::tests -- --nocapture`

Run: `cargo test --manifest-path src-tauri/Cargo.toml webview_cache::tests -- --nocapture`

Expected: FAIL because both modules are missing.

- [ ] **Step 3: Implement read-only usage and exact cache deletion**

Add `fs2 = "0.4"`. Use `fs2::available_space` for preflight. Keep all recursion private and symlink-aware. Cache removal accepts an already-resolved active WebView root and constructs exactly `root.join("WebKitCache")`; no caller-supplied delete path crosses the command boundary.

Implement persistent-profile copy classification that skips:

```text
WebKitCache
patina.db
patina.db-wal
patina.db-shm
backups
remote-backup-temp
api_token
```

Unknown or symlink entries must be reported, not overwritten or followed.

- [ ] **Step 4: Run focused tests and clippy**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_usage::tests -- --nocapture`

Run: `cargo test --manifest-path src-tauri/Cargo.toml webview_cache::tests -- --nocapture`

Expected: PASS.

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`

- [ ] **Step 5: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/platform/storage_usage.rs src-tauri/src/platform/webview_cache.rs src-tauri/src/platform/mod.rs
git commit -m "feat: add safe WebKit cache maintenance"
```

### Task 5: Migration Preview And Scheduling

**Files:**
- Create: `src-tauri/src/data/storage_migration.rs`
- Modify: `src-tauri/src/data/mod.rs`
- Modify: `src-tauri/src/data/sqlite_pool.rs`

- [ ] **Step 1: Write failing planner tests**

Cover absolute-path normalization, conflicting roots, existing databases, free-space margin, read-only preview, pending merge, cancellation, backup-before-pending, and WAL checkpoint ordering.

```rust
#[test]
fn data_and_webview_requests_merge_into_one_pending_plan() {
    let first = plan_pending(&current, None, Some(data_target), None).unwrap();
    let merged = plan_pending(&current, Some(&first), None, Some(webview_target)).unwrap();
    assert_eq!(merged.target_data_root, data_target);
    assert_eq!(merged.target_webview_root, webview_target);
}

#[test]
fn preview_does_not_create_target() {
    let preview = preview_with_deps(request, &deps).unwrap();
    assert!(!Path::new(&preview.target_data_root).exists());
}
```

- [ ] **Step 2: Verify the planner tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_migration::planner_tests -- --nocapture`

Expected: FAIL because migration planning is not implemented.

- [ ] **Step 3: Implement preview and schedule with injected side effects**

Keep pure validation/planning separate from Tauri-dependent orchestration. Scheduling must call, in order:

1. Existing backup export.
2. SQLite WAL checkpoint.
3. Atomic pending-document write.

Use a minimum free-space margin of `max(payload_bytes / 10, 64 MiB)`. A preview reports payload, available bytes, required bytes, source/target roots, and restart requirement.

- [ ] **Step 4: Run planner and existing backup tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_migration::planner_tests -- --nocapture`

Run: `cargo test --manifest-path src-tauri/Cargo.toml backup -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/data/storage_migration.rs src-tauri/src/data/mod.rs src-tauri/src/data/sqlite_pool.rs
git commit -m "feat: preview and schedule storage migration"
```

### Task 6: Restart-Time Migration, Validation, And Rollback

**Files:**
- Modify: `src-tauri/src/data/storage_migration.rs`
- Modify: `src-tauri/src/data/sqlite_pool.rs`

- [ ] **Step 1: Write failing executor integration tests**

Build temporary real SQLite databases using the current migrations. Test:

- Successful custom migration.
- Corrupt staged database rejection.
- Critical table count mismatch rejection.
- Missing source rejection without target creation.
- Anchor-write failure leaves source active.
- Existing custom target database rejection.
- Restore-default quarantine and rollback.
- Staging cleanup only with a matching marker.
- WebView persistent state copied without cache.

```rust
#[test]
fn failed_validation_keeps_source_and_does_not_write_anchor() {
    let result = execute_pending_with_deps(&pending, deps_with_corrupt_copy());
    assert!(result.is_err());
    assert!(source.join("patina.db").exists());
    assert!(!anchor_path.exists());
}
```

- [ ] **Step 2: Verify executor tests fail**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_migration::executor_tests -- --nocapture`

Expected: FAIL because restart-time execution is missing.

- [ ] **Step 3: Implement staged execution and rollback**

Expose only the startup entry point publicly:

```rust
pub async fn run_pending_storage_migration<R: Runtime>(app: &AppHandle<R>) -> Result<(), String>;
```

The executor must use a sibling staging directory, verify its marker before cleanup, open staged SQLite with `create_if_missing(false)`, run `PRAGMA integrity_check`, call current schema validation, compare critical row counts, quarantine conflicting default-owned payload, promote owned entries, then write anchors. Source data is never removed.

If an operation fails, record maintenance error, remove the pending operation to avoid a boot loop, and return `Ok(())` only when the unchanged active source can still be opened. Return `Err` when active custom storage itself is unavailable.

- [ ] **Step 4: Run executor tests and full Rust tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_migration::executor_tests -- --nocapture`

Expected: PASS.

Run: `cargo test --manifest-path src-tauri/Cargo.toml --quiet`

Expected: 0 failures.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/data/storage_migration.rs src-tauri/src/data/sqlite_pool.rs
git commit -m "feat: migrate storage safely during startup"
```

### Task 7: Runtime Path Integration And Thin Commands

**Files:**
- Create: `src-tauri/src/commands/storage.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/data/backup.rs`
- Modify: `src-tauri/src/data/remote_backup.rs`
- Modify: `src-tauri/src/data/sqlite_pool.rs`
- Modify: `src-tauri/src/app/main_window.rs`
- Modify: `src-tauri/src/app/widget.rs`
- Modify: `src-tauri/src/app/bootstrap.rs`
- Modify: `scripts/check-rust-boundaries.ts`

- [ ] **Step 1: Write failing integration-boundary tests**

Add tests that assert every persistent owner consumes `StoragePaths`, cache trim runs before WebView construction, and pending migration runs before SQLite initialization. Extend architecture checks if a source-level assertion is the practical boundary test.

- [ ] **Step 2: Verify the tests fail**

Run: `npm run check:rust-boundaries`

Run: `cargo test --manifest-path src-tauri/Cargo.toml storage_paths -- --nocapture`

Expected: at least one new assertion fails because old direct `app_paths` calls remain.

- [ ] **Step 3: Integrate resolved paths and commands**

Commands include snapshot, directory picker, preview/schedule for data and WebView roots, restore defaults, cancel pending, schedule cache clear, open directory, and restart. Opening directories must use a Linux-safe platform boundary (`tauri-plugin-opener` or `xdg-open` wrapper), not a shell string.

In bootstrap, call pending migration before `initialize_app_sqlite`. On unavailable anchored data, show a native `rfd::MessageDialog`, return the exact startup error, and do not initialize runtime services.

- [ ] **Step 4: Run boundary checks and Rust suite**

Run: `npm run check:rust-boundaries`

Run: `cargo test --manifest-path src-tauri/Cargo.toml --quiet`

Run: `cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings`

Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands src-tauri/src/data src-tauri/src/app src-tauri/src/platform src-tauri/src/domain scripts/check-rust-boundaries.ts
git commit -m "feat: wire custom storage into Linux runtime"
```

### Task 8: Frontend Storage Gateway And Action State

**Files:**
- Create: `src/platform/storage/storageRuntimeGateway.ts`
- Create: `src/features/settings/services/storagePathDisplay.ts`
- Create: `src/features/settings/services/storageSettingsActions.ts`
- Create: `src/features/settings/hooks/useStorageSettingsState.ts`
- Create: `tests/storageSettings.test.ts`
- Modify: `package.json`

- [ ] **Step 1: Write failing service tests**

Test formatting and dependency-injected flows without rendering React:

```typescript
await runTest("schedule flow previews before confirmation and mutation", async () => {
  const events: string[] = [];
  const result = await scheduleStorageMoveWithDeps("/mnt/data", {
    preview: async () => { events.push("preview"); return preview; },
    confirm: async () => { events.push("confirm"); return true; },
    schedule: async () => { events.push("schedule"); return pending; },
  });
  assert.deepEqual(events, ["preview", "confirm", "schedule"]);
  assert.equal(result.status, "scheduled");
});
```

Also test canceled confirmation, preview errors, pending cancellation, restore default, cache clear scheduling, and byte formatting.

- [ ] **Step 2: Verify the tests fail**

Run: `node --experimental-strip-types --experimental-specifier-resolution=node tests/storageSettings.test.ts`

Expected: FAIL because modules are missing.

- [ ] **Step 3: Implement the gateway, pure actions, and hook**

Keep Tauri command names inside `storageRuntimeGateway.ts`. The hook owns loading/busy/error/pending state and uses existing quiet confirm/toast facilities through injected callbacks. It must not merge storage actions into generic settings save state.

- [ ] **Step 4: Run focused tests and TypeScript build**

Run: `npm run test:storage`

Run: `npm run build`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/platform/storage src/features/settings/services/storagePathDisplay.ts src/features/settings/services/storageSettingsActions.ts src/features/settings/hooks/useStorageSettingsState.ts tests/storageSettings.test.ts package.json
git commit -m "feat: add storage settings runtime state"
```

### Task 9: Quiet Pro Settings Storage UI

**Files:**
- Create: `src/features/settings/components/SettingsStoragePanel.tsx`
- Modify: `src/features/settings/components/SettingsDataSafetyPanel.tsx`
- Modify: `src/features/settings/components/Settings.tsx`
- Modify: `src/features/settings/hooks/useSettingsPageState.ts`
- Modify: `src/features/settings/types.ts`
- Modify: `src/shared/copy/uiText.ts`
- Modify: `tests/uiSmoke.test.ts`
- Modify: `tests/uiBrowserSmoke.test.ts`

- [ ] **Step 1: Write failing UI smoke assertions**

Add fixture snapshots for default, custom, pending, and failure states. Assert that the panel exposes accessible action labels and never labels the whole WebView profile as disposable cache.

- [ ] **Step 2: Verify UI tests fail**

Run: `npm run test:ui-smoke`

Run: `npm run test:ui-browser-smoke`

Expected: FAIL because the storage panel and copy are missing.

- [ ] **Step 3: Implement the panel using existing primitives**

Use `QuietSubpanel`, `QuietActionRow`, existing buttons, status semantics, and Lucide icons. Provide clear default/custom badges, compact paths with tooltips, data/cache sizes, move/restore/open actions, pending cancellation, and cache-clear-on-restart. Do not nest cards, add new hardcoded colors, or make storage actions part of the main Settings save bar.

- [ ] **Step 4: Run Settings and UI tests**

Run: `npm run test:settings`

Run: `npm run test:storage`

Run: `npm run test:ui-smoke`

Run: `npm run test:ui-browser-smoke`

Run: `npm run build`

Expected: all pass with no overflow or overlap at tested desktop and compact widths.

- [ ] **Step 5: Commit**

```bash
git add src/features/settings src/shared/copy/uiText.ts tests/uiSmoke.test.ts tests/uiBrowserSmoke.test.ts
git commit -m "feat: add Linux storage controls to settings"
```

### Task 10: Documentation, Manual Safety Harness, And Full Verification

**Files:**
- Create: `scripts/storage-migration-smoke.ts` only if automated temp-root integration cannot cover the relaunch boundary.
- Modify: `docs/product-principles-and-scope.md`
- Modify: `docs/roadmap-and-prioritization.md`
- Modify: `docs/linux-development-setup.md`
- Modify: `docs/linux-port-and-api-design.md`
- Modify: `CHANGELOG.md` only when the release version is chosen.

- [ ] **Step 1: Add documentation validation assertions first**

Extend an existing documentation test or add focused assertions that active docs mention XDG defaults, fail-closed custom roots, retained old data, and exact `WebKitCache` cleanup.

- [ ] **Step 2: Verify documentation tests fail**

Run: `npm run test:agent-skill` or the selected focused documentation test.

Expected: FAIL until active docs are updated.

- [ ] **Step 3: Update long-lived documentation**

Update only active source-of-truth docs. Do not use or update archived Windows execution plans as current guidance. Record that custom storage is a Settings/Tauri capability, not an HTTP API endpoint.

- [ ] **Step 4: Run destructive-operation tests against temporary roots only**

Run focused Rust tests with temporary directories. Never point tests or smoke scripts at the real `${XDG_DATA_HOME}/Patina` directory.

Manually verify in a temporary development profile:

1. Preview does not mutate target.
2. Schedule produces backup and pending metadata.
3. Relaunch migrates a copied test database.
4. Custom root missing causes startup failure and no default DB creation.
5. Cache clear removes only test `WebKitCache`.
6. Restore default quarantines an existing default test DB.

- [ ] **Step 5: Run the full validation bar**

Run: `npm test`

Run: `npm run test:replay`

Run: `npm run build`

Run: `npm run check:rust`

Run: `npm run release:check`

Expected: all checks pass; Rust reports no failed tests or clippy warnings; frontend and browser smoke suites pass.

- [ ] **Step 6: Archive implementation documents after completion**

After the feature is implemented and verified, move this plan and the approved design into `docs/archive/` in the final implementation commit, because they are one-off execution documents rather than long-lived source-of-truth docs.

- [ ] **Step 7: Commit**

```bash
git add docs scripts tests package.json CHANGELOG.md
git commit -m "docs: complete Linux storage migration rollout"
```

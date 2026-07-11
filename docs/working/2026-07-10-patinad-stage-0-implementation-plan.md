# `patinad` Stage 0 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the current `patinad` skeleton into a profile-safe, storage-anchor-aware, single-owner daemon foundation that reuses the desktop HTTP transport and advertises only the API routes it actually serves.

**Architecture:** Keep executable entrypoints thin and move daemon assembly under `app/*`; reuse pure path/anchor functions from `platform/*`, schema work from `data/*`, and request handling from `engine/api/*`. Stage 0 does not own tracking yet and does not implement the browser UI, but its transport and ownership boundaries must support those later stages without a second implementation.

**Tech Stack:** Rust 2021, Tauri v2, Tokio, SQLx/SQLite, `fs2` advisory file locks, serde/serde_json, existing repository boundary scripts.

---

## Scope And Preconditions

The branch already contains an uncommitted Stage 1 experiment under `src-tauri/src/daemon/`, `src-tauri/src/bin/patinad.rs`, and `src-tauri/tests/daemon_status.rs`. Preserve useful behavior, but do not keep `daemon/*` as a long-term Rust root layer.

Stage 0 includes:

- Production / Local / Dev profile selection
- Tauri-free XDG root, anchor, and pending-migration resolution
- profile-correct API token path
- one background-runtime owner per profile
- one shared HTTP parser/response transport
- an API surface that keeps OpenAPI and real routes aligned
- health and OpenAPI routes only for daemon Stage 0

Stage 0 excludes:

- tracking, watchdog, power, audio, MPRIS, or browser bridge ownership
- browser UI assets, browser sessions, event streaming, TUI, systemd, or GPUI
- Windows source deletion
- new API write endpoints

## Target File Map

```text
src-tauri/src/
  bin/patinad.rs                 thin process entrypoint
  lib.rs                         thin public daemon entry function
  app/
    daemon/
      mod.rs                     Stage 0 assembly and lifecycle
      options.rs                 CLI/profile/port parsing
      status.rs                  startup status/readiness model
      storage.rs                 daemon storage bootstrap
      runtime.rs                 owned resources and graceful shutdown
    runtime_lease.rs             per-profile background owner lock
  platform/
    app_paths.rs                 environment roots and profile paths
    storage_paths.rs             Tauri-free anchored path resolution
  data/sqlite_pool.rs            open/prepare pool at an explicit path
  engine/api/
    auth.rs                      host-owned credential store
    http.rs                      shared bounded HTTP transport
    surface.rs                   enabled endpoint registry
    router.rs                    desktop and Stage 0 route dispatch
    server.rs                    listener lifecycle using shared transport
    handlers/openapi.rs          surface-aware OpenAPI document
```

The final code must remove `src-tauri/src/daemon/`. Unit tests should live with their owner modules; use `src-tauri/tests/daemon_status.rs` only for public binary/facade behavior that cannot be tested internally.

### Task 1: Move Daemon Assembly Under The App Owner Without Behavior Changes

**Files:**

- Create: `src-tauri/src/app/daemon/mod.rs`
- Create: `src-tauri/src/app/daemon/status.rs`
- Modify: `src-tauri/src/app/mod.rs`
- Modify: `src-tauri/src/bin/patinad.rs`
- Modify: `src-tauri/src/lib.rs`
- Preserve and include: `src-tauri/src/data/sqlite_pool.rs`
- Preserve and include: `src-tauri/src/engine/api/handlers/health.rs`
- Preserve and include: `src-tauri/src/engine/api/router.rs`
- Delete: `src-tauri/src/daemon/mod.rs`
- Modify or delete: `src-tauri/tests/daemon_status.rs`

- [ ] **Step 1: Run the existing daemon characterization tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test daemon_status
```

Expected: PASS. These tests are the behavior-preservation baseline for the owner relocation.

- [ ] **Step 2: Move the existing assembly without adding profile behavior**

Move the current structs and functions under `app/daemon/*`. Keep the current `serve_minimal_api` behavior temporarily. Do not report Dev while still resolving Production storage; profile parsing and storage resolution land atomically in Task 2.

Keep `lib.rs` thin:

```rust
pub fn run_daemon(
    args: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<(), String> {
    app::daemon::run(args)
}
```

Keep the binary limited to argument forwarding and exit-code handling. Move status construction to `app/daemon/status.rs`. Do not move SQL, HTTP parsing, or platform path details into `app/daemon/mod.rs`.

- [ ] **Step 3: Run characterization tests and binary checks**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test daemon_status
cargo check --manifest-path src-tauri/Cargo.toml --bins
```

Expected: PASS; both `patina` and `patinad` binaries compile.

- [ ] **Step 4: Commit the owner relocation**

```bash
git add src-tauri/src/app/mod.rs src-tauri/src/app/daemon/mod.rs src-tauri/src/app/daemon/status.rs src-tauri/src/bin/patinad.rs src-tauri/src/lib.rs src-tauri/src/data/sqlite_pool.rs src-tauri/src/engine/api/handlers/health.rs src-tauri/src/engine/api/router.rs src-tauri/tests/daemon_status.rs
git diff --cached --stat
git diff --cached --check
git commit -m "refactor: place patinad assembly under app owner"
```

### Task 2: Add Profile Options And Storage Resolution Atomically

**Files:**

- Modify: `src-tauri/src/platform/app_paths.rs`
- Modify: `src-tauri/src/platform/storage_paths.rs`
- Create: `src-tauri/src/app/daemon/options.rs`
- Create: `src-tauri/src/app/daemon/storage.rs`
- Modify: `src-tauri/src/app/daemon/mod.rs`
- Modify: `src-tauri/src/app/daemon/status.rs`
- Modify: `src-tauri/src/data/sqlite_pool.rs`

- [ ] **Step 1: Write failing option, path, and anchor tests**

Cover these cases with temporary roots:

```rust
#[test]
fn dev_profile_uses_dev_control_and_data_roots() { /* Patina Dev */ }

#[test]
fn anchored_data_root_is_used_without_tauri() { /* custom/patina.db */ }

#[test]
fn unavailable_custom_data_root_fails_closed() { /* no fallback */ }

#[test]
fn pending_migration_blocks_stage_zero_daemon_startup() {
    /* error tells the user to start the desktop app to finish maintenance */
}

#[test]
fn explicit_profile_and_ephemeral_port_are_parsed() {
    let options = DaemonRunOptions::from_args([
        "patinad", "--profile", "local", "--port", "0", "--serve-api",
    ]).unwrap();
    assert_eq!(options.profile, AppProfile::Local);
    assert_eq!(options.port_override, Some(0));
    assert!(options.serve_api);
}

#[test]
fn debug_build_defaults_to_dev_profile() { /* release defaults to Production */ }

#[test]
fn invalid_profile_is_rejected() { /* windows is not accepted */ }
```

The expected resolver shape is:

```rust
pub fn environment_roots() -> AppPathRoots;

pub fn resolve_storage_paths_for_profile(
    roots: &AppPathRoots,
    profile: AppProfile,
) -> Result<StoragePaths, String>;
```

- [ ] **Step 2: Run tests and verify RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml storage_paths -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml daemon::storage -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml daemon::options -- --nocapture
```

Expected: FAIL because the Tauri-free profile resolver does not exist.

- [ ] **Step 3: Implement environment roots and anchored resolution**

Use:

- `XDG_CONFIG_HOME`, otherwise `$HOME/.config`
- `XDG_DATA_HOME`, otherwise `$HOME/.local/share`
- `profile_paths()` for Production / Local / Dev folder names
- `read_data_anchor_from_dir()` and `read_webview_anchor_from_dir()`
- `resolve_storage_paths_from()` for validation and fail-closed behavior
- `read_pending_migration_from_dir()` to reject pending Stage 0 maintenance

Do not execute migration from daemon Stage 0. The existing desktop startup remains the only migration executor until a later design explicitly moves ownership.

Options, status, and resolved paths must be constructed as one unit. No intermediate commit may claim a profile while opening another profile's storage. Allow `--port 0` for repeatable development/tests; otherwise accept the same non-privileged port range as Local API settings.

- [ ] **Step 4: Honor `database_creation_allowed`**

The daemon must call:

```rust
open_prepared_sqlite_pool_at_path(
    &paths.db_path,
    paths.database_creation_allowed,
).await
```

Never pass `true` unconditionally. A custom anchored database must already exist.

- [ ] **Step 5: Verify path behavior**

```bash
cargo test --manifest-path src-tauri/Cargo.toml app_paths -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml storage_paths -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml daemon::storage -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml daemon::options -- --nocapture
```

Expected: PASS, including custom mount failure without creation of a default database.

- [ ] **Step 6: Commit storage bootstrap**

```bash
git add src-tauri/src/platform/app_paths.rs src-tauri/src/platform/storage_paths.rs src-tauri/src/app/daemon/options.rs src-tauri/src/app/daemon/storage.rs src-tauri/src/app/daemon/mod.rs src-tauri/src/app/daemon/status.rs src-tauri/src/data/sqlite_pool.rs
git diff --cached --stat
git diff --cached --check
git commit -m "fix: make patinad storage profile safe"
```

### Task 3: Make API Credentials Host-Owned And Profile-Correct

**Files:**

- Modify: `src-tauri/src/engine/api/auth.rs`
- Modify: `src-tauri/src/engine/api/configuration.rs`
- Modify: `src-tauri/src/app/runtime.rs`
- Modify: `src-tauri/src/app/daemon/mod.rs`
- Modify: `src-tauri/src/commands/diagnostics.rs`
- Modify: `src-tauri/src/commands/settings.rs`

- [ ] **Step 1: Write failing explicit-token-path tests**

Add tests that require initialization and rotation to stay on the active profile path:

```rust
#[test]
fn initializing_at_dev_path_does_not_write_production_path() { /* temp roots */ }

#[test]
fn rotation_reuses_the_active_token_path() { /* same path, new token */ }

#[test]
fn active_token_path_is_reported_to_diagnostics() { /* exact path */ }

#[test]
fn independent_credential_stores_never_cross_write() { /* dev and prod stores */ }
```

Tests must create independent stores rather than resetting process-global state. Keep file-format and permission tests pure where possible.

- [ ] **Step 2: Run and verify RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml engine::api::auth -- --nocapture
```

Expected: FAIL because auth currently derives one process-global Production path.

- [ ] **Step 3: Replace global credentials with a host-owned store**

Implement a cloneable store owned by each runtime host:

```rust
#[derive(Clone)]
pub struct ApiCredentialStore {
    inner: Arc<RwLock<ApiCredentialState>>,
}

struct ApiCredentialState {
    token: String,
    path: PathBuf,
}

impl ApiCredentialStore {
  pub fn initialize_at(
    &self,
    path: &Path,
    legacy_token: Option<&str>,
  ) -> Result<String, String>;
  pub fn validate(&self, authorization: Option<&str>) -> bool;
  pub fn rotate(&self) -> Result<String, String>;
  pub fn token_path(&self) -> Result<PathBuf, String>;
}
```

Rotation must atomically write to the store's configured path. Keep owner-only `0600` behavior and random 256-bit token tests. Avoid logging token values. Do not retain convenience globals that silently recreate the Production path.

- [ ] **Step 4: Pass the resolved path from both hosts**

The desktop host constructs and manages one `ApiCredentialStore` from `StoragePaths.api_token_path`; the daemon constructs its own store from Tauri-free bootstrap. Pass the store to API configuration, transport auth validation, diagnostics, and token rotation. Diagnostics and Settings call `store.token_path()` rather than recomputing a path from environment variables.

- [ ] **Step 5: Verify auth and settings tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml engine::api::auth -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml commands::diagnostics -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml commands::settings -- --nocapture
```

Expected: PASS; no token value appears in startup logs.

- [ ] **Step 6: Commit credential isolation**

```bash
git add src-tauri/src/engine/api/auth.rs src-tauri/src/engine/api/configuration.rs src-tauri/src/app/runtime.rs src-tauri/src/app/daemon/mod.rs src-tauri/src/commands/diagnostics.rs src-tauri/src/commands/settings.rs
git diff --cached --stat
git diff --cached --check
git commit -m "fix: isolate local API credentials by profile"
```

### Task 4: Enforce One Background Runtime Owner Per Profile

**Files:**

- Create: `src-tauri/src/app/runtime_lease.rs`
- Modify: `src-tauri/src/app/mod.rs`
- Modify: `src-tauri/src/app/bootstrap.rs`
- Modify: `src-tauri/src/app/daemon/mod.rs`
- Modify: `src-tauri/src/app/daemon/status.rs`

- [ ] **Step 1: Write failing lease tests**

Use a temporary control root and real `fs2` locks:

```rust
#[test]
fn second_owner_for_same_profile_is_rejected() { /* owner metadata returned */ }

#[test]
fn dropping_the_lease_allows_the_next_owner() { /* RAII unlock */ }

#[test]
fn separate_profile_control_roots_do_not_conflict() { /* dev vs production */ }
```

Test metadata must include role, PID, profile, and acquisition timestamp, but never secrets.

- [ ] **Step 2: Run and verify RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime_lease -- --nocapture
```

Expected: FAIL because `RuntimeLease` does not exist.

- [ ] **Step 3: Implement an advisory RAII lease**

Use the already-declared `fs2` dependency and a stable lock file below the profile control root:

```rust
pub struct RuntimeLease {
    file: std::fs::File,
    pub owner: RuntimeOwner,
}

pub fn acquire_runtime_lease(
    control_root: &Path,
    profile: AppProfile,
    role: RuntimeRole,
) -> Result<RuntimeLease, RuntimeLeaseError>;
```

Hold the file for the full host lifetime. On lock contention, read owner metadata for diagnostics. Do not use create-new lock files that remain stale after a crash.

- [ ] **Step 4: Acquire before storage maintenance and every database user**

Desktop order:

```text
resolve profile and stable control root
acquire RuntimeLease
run startup storage maintenance
resolve active anchored storage
initialize SQLite
start API/tracking runtime
```

Daemon Stage 0 follows the same order except it checks and rejects pending migration after acquiring the lease rather than executing it. This prevents desktop migration from racing a daemon database user. Store the desktop lease in managed app state so it cannot drop after setup.

Stage 0 remains experimental: if it acquires the lease, the desktop must fail with a clear owner message instead of starting a second tracker. Do not release Stage 0 as an autostart daemon yet.

- [ ] **Step 5: Verify lease and startup-order tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime_lease -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml startup_storage_maintenance_precedes_sqlite_initialization -- --nocapture
```

Expected: PASS; add or update a startup-order test that explicitly proves `lease acquisition < maintenance/pending check < SQLite initialization` for each host.

- [ ] **Step 6: Commit runtime ownership**

```bash
git add src-tauri/src/app/runtime_lease.rs src-tauri/src/app/mod.rs src-tauri/src/app/bootstrap.rs src-tauri/src/app/daemon/mod.rs src-tauri/src/app/daemon/status.rs
git diff --cached --stat
git diff --cached --check
git commit -m "feat: enforce one background runtime owner per profile"
```

### Task 5: Share One Bounded HTTP Transport

**Files:**

- Create: `src-tauri/src/engine/api/http.rs`
- Modify: `src-tauri/src/engine/api/mod.rs`
- Modify: `src-tauri/src/engine/api/router.rs`
- Modify: `src-tauri/src/engine/api/server.rs`
- Modify: `src-tauri/src/app/daemon/mod.rs`
- Delete: duplicate HTTP parser code from the current daemon experiment

- [ ] **Step 1: Write failing transport contract tests**

Use loopback `TcpListener` tests for:

- lowercase `authorization` and `content-length` headers
- malformed request returns 400
- request line/header section above configured limit returns 400 or 413
- body over 64 KiB returns 413 without allocation to the declared size
- stalled headers time out and close
- valid bearer token reaches the provided route handler
- missing/invalid token returns 401
- an unauthenticated `OPTIONS` request returns 204 with an empty body and documented CORS headers without invoking auth validation or a business handler
- all non-`OPTIONS` data routes still require bearer auth

- [ ] **Step 2: Run and verify RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml engine::api::http -- --nocapture
```

Expected: FAIL because no shared bounded transport exists.

- [ ] **Step 3: Implement request and response transport**

Use a transport-owned request type:

```rust
pub struct ApiRequest {
    pub method: String,
    pub path: String,
    pub query: Option<String>,
    pub body: Vec<u8>,
    pub authorization: Option<String>,
}
```

Expose one async connection runner that receives an auth validator and route closure. Header names are case-insensitive. Apply one timeout to request parsing and explicit limits before allocating the body.

`engine/api/server.rs` remains the only owner of bind, accept loop, per-connection spawn, shutdown signal, connection task set, and listener task join. It exposes a host-neutral prepared listener/server handle; desktop and daemon provide only credentials, `ApiSurface`, route context, and lifecycle assembly.

Track connection tasks with `tokio::task::JoinSet` or an equivalent owned set. Shutdown stops accept first, then drains completed/in-flight connections up to a bounded timeout, aborts any remaining stalled tasks, and joins them before the server handle resolves. The server must never return shutdown completion while a detached request can still access runtime state.

Do not add axum or another framework in Stage 0. The current needs are small, and a framework migration would enlarge the change beyond the approved scope.

Keep the current documented permissive localhost CORS contract in Stage 0; do not broaden listener addresses or add credential cookies. Browser UI same-origin sessions and stricter Origin policy belong to the later browser-client plan.

- [ ] **Step 4: Remove daemon transport duplication**

`app/daemon/mod.rs` must not bind/accept connections, parse HTTP, format status lines, validate bearer tokens, or serialize JSON responses. It asks `engine/api/server.rs` to start a prepared server and retains the returned handle for later shutdown.

- [ ] **Step 5: Verify desktop and daemon transport behavior**

```bash
cargo test --manifest-path src-tauri/Cargo.toml engine::api::http -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml engine::api::server -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml daemon -- --nocapture
```

Expected: PASS; one parser implementation serves both hosts. Include a stalled-connection shutdown test that proves the server handle resolves and no connection task remains detached.

- [ ] **Step 6: Commit shared transport**

```bash
git add src-tauri/src/engine/api/http.rs src-tauri/src/engine/api/mod.rs src-tauri/src/engine/api/router.rs src-tauri/src/engine/api/server.rs src-tauri/src/app/daemon/mod.rs
git diff --cached --stat
git diff --cached --check
git commit -m "refactor: share bounded local API transport"
```

### Task 6: Align Enabled Routes And OpenAPI

**Files:**

- Create: `src-tauri/src/engine/api/surface.rs`
- Modify: `src-tauri/src/engine/api/mod.rs`
- Modify: `src-tauri/src/engine/api/router.rs`
- Modify: `src-tauri/src/engine/api/handlers/openapi.rs`
- Modify: `src-tauri/src/app/daemon/mod.rs`
- Modify: `src-tauri/src/app/daemon/status.rs`

- [ ] **Step 1: Write failing surface tests**

```rust
#[test]
fn stage_zero_surface_contains_only_health_and_openapi() { /* exact set */ }

#[test]
fn stage_zero_openapi_does_not_advertise_sessions() { /* no /sessions */ }

#[test]
fn every_advertised_stage_zero_method_and_path_returns_200_with_valid_auth() {
    /* extract OpenAPI operations and route each one */
}

#[test]
fn stage_zero_routes_and_openapi_are_bidirectionally_equal() {
    /* advertised subset routable AND public routable subset advertised */
}

#[test]
fn desktop_surface_keeps_existing_method_and_path_set() { /* exact regression set */ }
```

- [ ] **Step 2: Run and verify RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml api::surface -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml api::handlers::openapi -- --nocapture
```

Expected: FAIL because OpenAPI currently always emits the full desktop path map.

- [ ] **Step 3: Implement explicit API surfaces**

Use a small enum, not a plugin registry:

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ApiSurface {
    Desktop,
    DaemonStage0,
}
```

The surface owns enabled endpoint descriptors. Router matching and OpenAPI path filtering must consume the same descriptors or have a bidirectional test that enforces exact method/path agreement. A 401, 404, or 500 does not count as a successfully routed advertised Stage 0 operation; health and OpenAPI must return 200 with valid auth.

Keep dynamic app action paths on the Desktop surface. Stage 0 exposes only:

```text
GET /api/v1/health
GET /api/v1/openapi.json
```

- [ ] **Step 4: Make daemon status truthful**

When `--serve-api` is active and listener preparation succeeds, status reports API enabled and the confirmed port. Without `--serve-api`, it reports disabled. Tracking remains false in every Stage 0 status.

- [ ] **Step 5: Verify surface, router, and OpenAPI tests**

```bash
cargo test --manifest-path src-tauri/Cargo.toml api::surface -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml api::handlers::openapi -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml daemon -- --nocapture
```

Expected: PASS; no Stage 0 document advertises an unavailable endpoint.

- [ ] **Step 6: Commit API capability alignment**

```bash
git add src-tauri/src/engine/api/surface.rs src-tauri/src/engine/api/mod.rs src-tauri/src/engine/api/router.rs src-tauri/src/engine/api/handlers/openapi.rs src-tauri/src/app/daemon/mod.rs src-tauri/src/app/daemon/status.rs
git diff --cached --stat
git diff --cached --check
git commit -m "fix: align patinad routes with OpenAPI"
```

### Task 7: Own And Gracefully Release Daemon Resources

**Files:**

- Create: `src-tauri/src/app/daemon/runtime.rs`
- Modify: `src-tauri/src/app/daemon/mod.rs`
- Modify: `src-tauri/src/engine/api/server.rs`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Write failing lifecycle tests**

Build a daemon runtime from temporary storage, a random listener port, and a runtime lease. Require explicit shutdown to prove:

```rust
#[tokio::test]
async fn graceful_shutdown_releases_listener_pool_and_lease() {
    /* start runtime */
    /* keep one connection stalled/in-flight */
    /* shutdown().await */
    /* bind same port, open DB, and acquire same lease again */
}

#[tokio::test]
async fn shutdown_is_safe_when_api_was_not_started() { /* no leaked pool/lease */ }
```

- [ ] **Step 2: Run and verify RED**

```bash
cargo test --manifest-path src-tauri/Cargo.toml daemon::runtime -- --nocapture
```

Expected: FAIL because there is no owned `DaemonRuntime` or awaitable server shutdown handle.

- [ ] **Step 3: Implement explicit ownership and shutdown order**

```rust
pub struct DaemonRuntime {
    api_server: Option<ApiServerHandle>,
    sqlite: Option<DaemonSqliteRuntime>,
    lease: Option<RuntimeLease>,
}

impl DaemonRuntime {
    pub async fn shutdown(mut self) {
        if let Some(server) = self.api_server.take() {
            server.shutdown().await;
        }
        if let Some(sqlite) = self.sqlite.take() {
            sqlite.pool.close().await;
        }
        drop(self.lease.take());
    }
}
```

The API server stops accepting, drains or aborts its bounded connection task set, and joins every connection task before SQLite closes; SQLite closes before the runtime lease is released. Avoid relying on process termination or `Drop` running async work.

- [ ] **Step 4: Add CLI signal handling**

Enable Tokio's `signal` feature in `src-tauri/Cargo.toml`. When Stage 0 serves the API, await `tokio::signal::ctrl_c()`, then call `DaemonRuntime::shutdown().await`. A non-serving diagnostic run may print status and immediately call the same shutdown path.

- [ ] **Step 5: Verify lifecycle and binary behavior**

```bash
cargo test --manifest-path src-tauri/Cargo.toml daemon::runtime -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml engine::api::server -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml --bins
```

Expected: PASS; shutdown permits immediate port and lease reacquisition.

- [ ] **Step 6: Commit lifecycle ownership**

```bash
git add src-tauri/Cargo.toml src-tauri/src/app/daemon/runtime.rs src-tauri/src/app/daemon/mod.rs src-tauri/src/engine/api/server.rs
git diff --cached --stat
git diff --cached --check
git commit -m "feat: add graceful patinad shutdown"
```

### Task 8: Stage 0 Integration Verification And Documentation

**Files:**

- Create: `src-tauri/tests/daemon_stage_zero.rs`
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Modify: `docs/working/2026-07-10-patinad-runtime-design.md`
- Modify if behavior changed: `docs/api-index.md`

- [ ] **Step 1: Add an end-to-end Stage 0 test**

The test must use temporary XDG roots and a random port. It must prove:

1. Dev profile creates/opens only `Patina Dev`.
2. An anchored existing database is used without creating default `patina.db`.
3. A pending migration rejects startup.
4. A second lease owner is rejected with diagnostics.
5. Authenticated health and OpenAPI requests succeed.
6. OpenAPI contains only the two Stage 0 routes.
7. Shutdown releases listener, SQLite pool, and lease.

- [ ] **Step 2: Run focused Stage 0 verification**

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test --manifest-path src-tauri/Cargo.toml daemon -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml --bins
npm run check:rust-boundaries
```

Expected: all commands PASS.

- [ ] **Step 3: Run the repository architecture validation bar**

```bash
npm run check:full
```

Expected: PASS. If an unrelated existing failure occurs, record the exact command and failure; do not weaken a boundary rule to make Stage 0 pass.

- [ ] **Step 4: Perform a safe manual smoke test**

Use unique temporary XDG roots and an ephemeral port so repeated runs cannot reuse state or conflict with the desktop API:

```bash
stage0_root="$(mktemp -d -t patina-stage0-XXXXXX)"
XDG_CONFIG_HOME="$stage0_root/config" \
XDG_DATA_HOME="$stage0_root/data" \
cargo run --manifest-path src-tauri/Cargo.toml --bin patinad -- \
  --profile dev --port 0 --serve-api
```

Verify startup prints the Dev data path and confirmed ephemeral port, but no token value. Call health/OpenAPI with the token from the temporary Dev token file. Send Ctrl+C, confirm graceful shutdown, then remove `stage0_root`.

- [ ] **Step 5: Update current-state documentation**

Keep README status as development-only. Record Stage 0 safety guarantees in the active design, but do not claim tracking ownership, browser UI, systemd, or stable daemon release.

- [ ] **Step 6: Commit Stage 0 verification and docs**

```bash
git add README.md README.zh-CN.md docs/working/2026-07-10-patinad-runtime-design.md docs/api-index.md src-tauri/tests/daemon_stage_zero.rs
git diff --cached --stat
git diff --cached --check
git commit -m "test: verify patinad stage zero foundation"
```

## Completion Criteria

Stage 0 is complete only when all of the following are true:

- `cargo run --bin patinad` in a debug build defaults to Dev, never Production
- explicit Local and Production profiles resolve their own control/data/token paths
- custom anchored data is honored and missing custom storage fails closed
- pending migration cannot be bypassed by daemon startup
- desktop and daemon cannot both become the background runtime owner for one profile
- desktop and daemon use one bounded HTTP transport
- daemon OpenAPI exactly matches its two enabled routes
- no token value is logged or placed in a URL
- tracking remains desktop-owned and Stage 0 is not installed as an autostart service
- `npm run check:full` passes

## Next Plan Boundary

After Stage 0 is verified and committed, write a separate Stage 1 plan for `RuntimeContext`, `RuntimeEventSink`, full read-only API, and the transport-neutral frontend gateway. Browser UI assets and event streaming belong to that later plan, not this one.

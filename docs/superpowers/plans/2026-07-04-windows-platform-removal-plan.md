# Windows Platform Removal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Remove unsupported Windows platform code, dependencies, packaging resources, and active documentation while preserving Linux and data-format behavior.

**Architecture:** Make the Rust application explicitly Linux-only, replace conditional Windows/Linux aliases with direct Linux platform owners, then delete Windows modules and target dependencies. Protect the new boundary with release and Rust checks instead of relying on convention.

**Tech Stack:** Rust, Cargo target dependencies, Tauri 2, Node boundary scripts, Linux CI.

**Reference:** `docs/superpowers/specs/2026-07-04-patina-linux-independent-project-transition-design.md`

---

### Task 1: Add a Failing Engine and Command Boundary Contract

**Files:**
- Modify: `tests/releasePolicy.test.ts`
- Modify: `scripts/check-rust-boundaries.ts`

- [ ] **Step 1: Add assertions that reject Windows imports in engine and commands**

Extend the Rust boundary check to reject production imports of `crate::platform::windows` from `src-tauri/src/engine` and `src-tauri/src/commands`. Do not yet assert that the platform directory or Cargo dependencies are absent.

- [ ] **Step 2: Run and verify RED**

Run: `npm run test:release && npm run check:rust-boundaries`

Expected: FAIL and list current engine/command imports.

- [ ] **Step 3: Keep the failing check for Task 2's RED-GREEN cycle**

### Task 2: Make Tracking and Commands Directly Linux-Owned

**Files:**
- Modify: `src-tauri/src/engine/tracking/*.rs`
- Modify: `src-tauri/src/engine/tracking/runtime/*.rs`
- Modify: `src-tauri/src/commands/tracking.rs`
- Modify: `src-tauri/src/commands/apps.rs`
- Modify: `src-tauri/src/commands/settings.rs`
- Modify: `src-tauri/src/commands/widget.rs`
- Modify: `src-tauri/src/commands/diagnostics.rs`

- [ ] **Step 1: Add or update Linux-focused unit tests before each alias change**

Ensure foreground `WindowInfo`, audio/media participation, icons, diagnostics, AFK threshold, and widget input behavior remain covered.

- [ ] **Step 2: Replace conditional tracker aliases**

Import `crate::platform::linux::*` directly. Remove Windows branches without changing Linux state-machine or command payload contracts.

- [ ] **Step 3: Run focused Rust tests and the boundary check**

Run tracking, sustained-participation, command, and diagnostics test filters after each small group. Finish with `npm run check:rust-boundaries`; it must now pass.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/engine src-tauri/src/commands scripts/check-rust-boundaries.ts
git commit -m "refactor: make tracking runtime Linux-owned"
```

### Task 3: Remove Windows App and Platform Integration

**Files:**
- Delete: `src-tauri/src/platform/windows/`
- Modify: `src-tauri/src/platform/mod.rs`
- Modify: `src-tauri/src/platform/credentials.rs`
- Modify: `src-tauri/src/app/main_window.rs`
- Modify: `src-tauri/src/app/runtime.rs`
- Modify: `src-tauri/src/engine/tools/notification.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add failing tests for the platform/app removal boundary**

Require `src-tauri/src/platform/windows` to be absent and reject production imports from `src-tauri/src/app`. Run the focused boundary tests and verify RED.

- [ ] **Step 2: Add a Linux-only compile contract**

Add an explicit non-Linux `compile_error!` at the crate boundary so unsupported targets fail with a clear message.

- [ ] **Step 3: Remove Windows activation, power, credentials, notification, and resource branches**

Keep Linux implementations and cross-platform data compatibility. Do not delete old backup/schema aliases merely because they originated on Windows.

- [ ] **Step 4: Delete the Windows platform directory and module registration**

- [ ] **Step 5: Run Rust verification and make the new boundary tests GREEN**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo check --manifest-path src-tauri/Cargo.toml --quiet
cargo test --manifest-path src-tauri/Cargo.toml --quiet
```

- [ ] **Step 6: Commit**

```bash
git add -A src-tauri/src tests/releasePolicy.test.ts scripts/check-rust-boundaries.ts
git commit -m "refactor: remove Windows platform runtime"
```

### Task 4: Remove Windows Dependencies and Bundle Assets

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/Cargo.lock`
- Modify: `src-tauri/tauri.conf.json`
- Delete where unreferenced: `src-tauri/icons/icon.ico`, `src-tauri/icons/icon.icns`, `src-tauri/icons/Square*`, `src-tauri/icons/StoreLogo.png`
- Modify: release and boundary tests

- [ ] **Step 1: Add failing package/config boundary assertions**

Require no Windows target dependency section, no Windows bundle block, Linux bundle targets only, and no references to Windows/macOS icon formats. Run `npm run test:release` and verify RED.

- [ ] **Step 2: Remove Windows target dependencies and simplify single-instance target scope to Linux**

Remove direct `windows` and `tauri-winrt-notification` dependencies. Do not assert that Cargo.lock contains no transitive Windows crates; cross-platform dependencies may legitimately retain them.

- [ ] **Step 3: Remove Windows bundle configuration and unreferenced icons**

Set bundle targets to AppImage and DEB. Keep only Linux PNG icons actually consumed by Tauri and the UI.

- [ ] **Step 4: Refresh Cargo.lock and run checks**

Run `cargo check`, Clippy with `-D warnings`, tests, and `npm run test:release`.

- [ ] **Step 5: Commit**

```bash
git add -A src-tauri tests scripts
git commit -m "build: remove Windows dependencies and assets"
```

### Task 5: Align Active Long-Term Documentation

**Files:**
- Modify: `docs/product-principles-and-scope.md`
- Modify: `docs/roadmap-and-prioritization.md`
- Modify: `docs/architecture.md`
- Modify: `docs/engineering-quality.md`
- Modify: `docs/versioning-and-release-policy.md`
- Modify: `docs/linux-port-and-api-design.md`
- Modify: `README.md`
- Modify: `README.zh-CN.md`

- [ ] **Step 1: Add failing doc assertions for stale Windows-retention claims**

Reject active statements such as “Windows source remains as historical compatibility code”. Do not scan `docs/archive`.

- [ ] **Step 2: Update source-of-truth docs**

State that Patina Linux is Linux-only and that upstream Windows changes are reference input only. Preserve MIT attribution.

- [ ] **Step 3: Make boundary checks GREEN**

Run:

```bash
npm run test:release
npm run check:rust-boundaries
npm run release:check
git diff --check
```

- [ ] **Step 4: Commit and push**

```bash
git add docs README.md README.zh-CN.md tests scripts
git commit -m "docs: define the Linux-only maintenance boundary"
git push origin main
```

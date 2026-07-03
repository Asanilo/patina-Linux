# Patina Linux Identity Transition Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Present the product as Patina Linux while preserving every deployed Linux package, data, updater, extension, API, and autostart identity.

**Architecture:** Keep technical package identity and storage paths stable, change only the Tauri identifiers and user-visible branding. Centralize display-name ownership in small frontend and Rust domain modules, and verify the new identifiers still resolve the existing `Patina` profile paths before changing configuration.

**Tech Stack:** React 19, TypeScript, Rust, Tauri 2, GNOME Shell extension, Chromium MV3, Firefox WebExtension, Node test scripts.

**Reference:** `docs/superpowers/specs/2026-07-04-patina-linux-independent-project-transition-design.md`

---

### Task 1: Change Identifiers Without Moving Data

**Files:**
- Modify: `src-tauri/src/platform/app_paths.rs`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `src-tauri/tauri.local.conf.json`
- Modify: `src-tauri/tauri.dev.conf.json`
- Test: `tests/releasePolicy.test.ts`

- [ ] **Step 1: Add failing Rust tests for all three new identifiers**

Change `resolves_profile_from_current_identifiers` to expect `io.github.asanilo.patinalinux`, `.local`, and `.dev`. Keep assertions that all resolved roots end in `Patina`, `Patina Local`, or `Patina Dev`.

- [ ] **Step 2: Run the focused Rust test and verify RED**

Run: `cargo test --manifest-path src-tauri/Cargo.toml platform::app_paths::tests -- --nocapture`

Expected: FAIL because the current constants still use `com.ceceliaee.patina`.

- [ ] **Step 3: Update only the identifier constants**

Use:

```rust
pub const IDENTIFIER_PROD: &str = "io.github.asanilo.patinalinux";
pub const IDENTIFIER_LOCAL: &str = "io.github.asanilo.patinalinux.local";
pub const IDENTIFIER_DEV: &str = "io.github.asanilo.patinalinux.dev";
```

Do not change `PRODUCT_FOLDER*`.

- [ ] **Step 4: Add failing release-policy assertions for config identity stability**

Assert all three JSON configs use the new identifiers, while production still has:

```ts
assert.equal(config.productName, "Patina");
assert.equal(config.mainBinaryName, "Patina");
```

- [ ] **Step 5: Update the three Tauri configs and verify GREEN**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml platform::app_paths::tests -- --nocapture
npm run test:release
```

Expected: all focused tests pass and no path assertion changes.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/platform/app_paths.rs src-tauri/tauri.conf.json src-tauri/tauri.local.conf.json src-tauri/tauri.dev.conf.json tests/releasePolicy.test.ts
git commit -m "refactor: adopt Patina Linux application identifiers"
```

### Task 2: Centralize the User-Visible Product Name

**Files:**
- Create: `src/shared/productIdentity.ts`
- Create: `src-tauri/src/domain/product_identity.rs`
- Modify: `src-tauri/src/domain/mod.rs`
- Modify: `src/app/components/AppTitleBar.tsx`
- Modify: `src/features/about/components/AboutPanel.tsx`
- Modify: `src-tauri/src/app/main_window.rs`
- Modify: `src-tauri/src/app/widget.rs`
- Modify: `src-tauri/src/app/tray.rs`
- Modify: `src-tauri/src/engine/tools/notification.rs`
- Test: `tests/uiSmoke.test.ts`
- Test: Rust unit tests colocated with `product_identity.rs`

- [ ] **Step 1: Add failing frontend identity tests**

Import the proposed `PRODUCT_DISPLAY_NAME` and assert it equals `Patina Linux`. Assert AppTitleBar and AboutPanel reference the identity module instead of defining `"Patina"` locally.

- [ ] **Step 2: Run UI smoke and verify RED**

Run: `npm run test:ui-smoke`

Expected: FAIL because the identity module does not exist.

- [ ] **Step 3: Add the frontend identity owner and update consumers**

Create:

```ts
export const PRODUCT_DISPLAY_NAME = "Patina Linux";
```

This shared module owns one stable cross-feature product fact. Do not add package names, paths, or protocol IDs to it.

- [ ] **Step 4: Add a failing Rust identity test**

Define the desired constants in the test contract:

```rust
assert_eq!(DISPLAY_NAME, "Patina Linux");
assert_eq!(PACKAGE_NAME, "Patina");
```

- [ ] **Step 5: Add the Rust domain identity owner and update consumers**

Use `DISPLAY_NAME` for main window, widget, tray tooltip, and notifications. Keep `PACKAGE_NAME` only where package compatibility is relevant.

- [ ] **Step 6: Run focused tests and verify GREEN**

Run:

```bash
npm run test:ui-smoke
cargo test --manifest-path src-tauri/Cargo.toml product_identity -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml --quiet
```

- [ ] **Step 7: Commit**

```bash
git add src/shared/productIdentity.ts src-tauri/src/domain/product_identity.rs src-tauri/src/domain/mod.rs src/app/components/AppTitleBar.tsx src/features/about/components/AboutPanel.tsx src-tauri/src/app/main_window.rs src-tauri/src/app/widget.rs src-tauri/src/app/tray.rs src-tauri/src/engine/tools/notification.rs tests/uiSmoke.test.ts
git commit -m "feat: present the app as Patina Linux"
```

### Task 3: Brand Linux Desktop and Release Surfaces

**Files:**
- Create: `src-tauri/patina.desktop.hbs`
- Modify: `src-tauri/tauri.conf.json`
- Modify: `.github/workflows/prepare-release.yml`
- Modify: `scripts/release.ts`
- Test: `tests/releasePolicy.test.ts`

- [ ] **Step 1: Add failing release contract assertions**

Require the desktop template to contain `Name=Patina Linux`, and require the workflow release title to use `Patina Linux v...`. Continue asserting bundle filenames are `Patina_<version>_amd64.*`.

- [ ] **Step 2: Run release tests and verify RED**

Run: `npm run test:release`

- [ ] **Step 3: Add the desktop template**

Use Tauri's documented Handlebars variables:

```ini
[Desktop Entry]
Categories={{categories}}
{{#if comment}}Comment={{comment}}{{/if}}
Exec={{exec}}
Icon={{icon}}
Name=Patina Linux
Terminal=false
Type=Application
```

Configure `bundle.linux.deb.desktopTemplate` to this file. Do not change `productName` or `mainBinaryName`.

- [ ] **Step 4: Update release titles without renaming assets**

Change only the GitHub Release display title and generated release-note heading to `Patina Linux`. Keep all asset names stable.

- [ ] **Step 5: Verify the generated Debian desktop entry**

Run:

```bash
npm run tauri build -- --bundles deb --config '{"bundle":{"createUpdaterArtifacts":false}}'
npm run test:release
```

Extract the generated DEB with `dpkg-deb -x` into `/tmp`, then verify the installed `.desktop` has `Name=Patina Linux` and still executes `Patina`.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/patina.desktop.hbs src-tauri/tauri.conf.json .github/workflows/prepare-release.yml scripts/release.ts tests/releasePolicy.test.ts
git commit -m "feat: brand Linux releases as Patina Linux"
```

### Task 4: Update Extension Display Names Without Changing IDs

**Files:**
- Modify: `extensions/gnome-shell/patina-window-tracker@patina/metadata.json`
- Modify: `extensions/chromium/manifest.json`
- Modify: `extensions/firefox/manifest.json`
- Modify: extension HTML, localization, README, store-listing, and privacy files containing the old display name
- Modify after signing: `extensions/firefox/dist/patina-web-sync.xpi`
- Test: extension check scripts and existing extension tests

- [ ] **Step 1: Add failing checks for display names, stable IDs, and new versions**

Require `Patina Linux Window Tracker` and `Patina Linux Web Sync`, while asserting the GNOME UUID and Firefox Gecko ID are unchanged. Require version bumps to GNOME `3`, Chromium `0.1.1`, and Firefox `0.1.2` so installed extensions can receive the renamed packages.

- [ ] **Step 2: Run extension checks and verify RED**

Run:

```bash
npm run extension:gnome:check
npm run extension:chromium:check
npm run extension:firefox:check
```

- [ ] **Step 3: Update source display strings and extension versions**

Do not change local ports, permissions, UUIDs, Gecko ID, or request payloads. Version changes are limited to the three values defined by the failing tests.

- [ ] **Step 4: Rebuild and sign Firefox XPI**

Build the unsigned XPI, sign it through the existing signing process, and replace the tracked signed XPI. Never commit signing credentials.

- [ ] **Step 5: Verify packaged extensions**

Run:

```bash
npm run extension:gnome:check
npm run extension:chromium:check
npm run extension:firefox:verify-signed
```

- [ ] **Step 6: Commit**

Commit source and verified signed XPI together so the release cannot publish mismatched Firefox metadata.

### Task 5: Replace Fork Language and Preserve Attribution

**Files:**
- Modify: `README.md`
- Modify: `README.zh-CN.md`
- Modify: `docs/product-principles-and-scope.md`
- Modify: `docs/roadmap-and-prioritization.md`
- Modify: `docs/versioning-and-release-policy.md`
- Modify: active API/MCP/Agent Skill docs where the product display name is user-facing
- Test: `tests/patinaIntegrationDocs.test.ts`
- Test: `tests/releasePolicy.test.ts`

- [ ] **Step 1: Add failing documentation assertions**

Require the README heading to be `Patina Linux`, reject `Patina Linux Fork`, and require explicit upstream attribution to Ceceliaee/Patina and MIT.

- [ ] **Step 2: Run documentation tests and verify RED**

Run: `npm run test:mcp && npm run test:release`

- [ ] **Step 3: Update active documentation**

Describe the project as independently maintained and Linux-first. Do not remove upstream attribution or rewrite archived historical plans.

- [ ] **Step 4: Run the identity validation set**

Run:

```bash
npm run test:release
npm run test:mcp
npm run test:agent-skill
npm run release:check
git diff --check
```

- [ ] **Step 5: Commit and push the completed identity batch**

Stage only the README, active long-lived docs, and integration tests changed by this task. Commit with `docs: establish Patina Linux project identity`, then push `origin/main` after `git status --short` confirms no unrelated files are staged.

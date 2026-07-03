# Linux Package Size Optimization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reduce at least one Linux installation package by 5% from v1.8.3 without growing the other by more than 2% or weakening runtime compatibility.

**Architecture:** Measure extracted package contents first, then retain only evidence-backed changes. Apply release-profile and resource changes one variable at a time, rebuild both packages after each experiment, and keep a change only when size and full validation improve together.

**Tech Stack:** Tauri 2 bundler, Cargo release profiles, AppImage extraction, dpkg-deb, Node.js analysis script.

**Reference:** `docs/superpowers/specs/2026-07-04-patina-linux-independent-project-transition-design.md`

---

### Task 1: Add Reproducible Bundle Size Reports

**Files:**
- Create: `scripts/analyze-linux-bundle.ts`
- Create: `tests/linuxBundleAnalysis.test.ts`
- Modify: `package.json`
- Create during execution: `docs/working/v1.9.0-linux-package-size-report.md`

- [ ] **Step 1: Write failing tests for deterministic directory reports**

Use a temporary fixture tree. Require total bytes, sorted largest files, extension/group totals, package metadata, and comparison percentages. Reject missing packages and malformed baseline values.

- [ ] **Step 2: Run and verify RED**

Run: `node --experimental-strip-types tests/linuxBundleAnalysis.test.ts`

- [ ] **Step 3: Implement the analyzer**

The script accepts extracted AppImage/DEB directories plus original package file paths and emits JSON and Markdown. It must not extract or delete paths itself.

- [ ] **Step 4: Add package commands and verify GREEN**

Include the focused test in `check:frontend`.

- [ ] **Step 5: Commit**

```bash
git add scripts/analyze-linux-bundle.ts tests/linuxBundleAnalysis.test.ts package.json
git commit -m "feat: add Linux bundle size analysis"
```

### Task 2: Record the v1.8.3 Baseline

- [ ] **Step 1: Download official v1.8.3 DEB and AppImage into a temporary analysis directory**

Verify expected sizes: AppImage `91,052,536`, DEB `13,406,882` bytes.

- [ ] **Step 2: Extract without executing application code**

Use `dpkg-deb -x` for DEB and AppImage extraction mode in a dedicated temporary directory.

- [ ] **Step 3: Generate the baseline report**

Record package totals, release binary size, bundled libraries, frontend dist, icons, extension resources, and top files.

- [ ] **Step 4: Commit only the redacted report**

Do not commit downloaded binaries or extracted bundle trees.

### Task 3: Test Release Symbol Stripping

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: package size report

- [ ] **Step 1: Build an unsigned local candidate before changing the profile**

Build both bundles with `createUpdaterArtifacts=false`, extract, and record current-branch baseline.

- [ ] **Step 2: Add `strip = "symbols"` to `[profile.release]`**

Do not combine LTO or panic changes in this step.

- [ ] **Step 3: Rebuild, compare, and validate**

Keep stripping only if both packages start correctly and at least one decreases without material regression.

- [ ] **Step 4: Run `npm run release:check` and commit**

Commit the profile change and report evidence together.

### Task 4: Test Thin LTO Independently

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: package size report

- [ ] **Step 1: Add only `lto = "thin"` as one documented experiment**

- [ ] **Step 2: Rebuild both bundles and compare against the retained stripped baseline**

- [ ] **Step 3: Run startup, tracker, tray, window, API, and updater checks**

- [ ] **Step 4: Keep or revert based on evidence**

Do not retain a change that only increases build time without meaningful package reduction.

### Task 5: Test Codegen Units Independently

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Modify: package size report

- [ ] **Step 1: Add only `codegen-units = 1` on top of the retained profile baseline**

- [ ] **Step 2: Rebuild both bundles and compare size plus build time**

- [ ] **Step 3: Keep or revert based on measured benefit**

- [ ] **Step 4: Run `npm run release:check` if retained**

### Task 6: Remove Only Proven-Unused Linux Bundle Resources

**Files:**
- Modify only files identified by the baseline report and reference search
- Modify: package size report

- [ ] **Step 1: Trace every candidate through Tauri config, Rust includes, Vite imports, and package contents**

- [ ] **Step 2: Add a failing reference/bundle assertion before removal**

- [ ] **Step 3: Remove one resource/dependency group at a time**

- [ ] **Step 4: Rebuild and verify after each group**

Never remove AppImage compatibility libraries solely because they are large.

### Task 7: Enforce and Document the Size Result

- [ ] **Step 1: Generate final candidate reports**

- [ ] **Step 2: Assert the 5% / 2% acceptance rule**

At least one final package must be at most 95% of its v1.8.3 byte size; the other must be at most 102%.

- [ ] **Step 3: Run full validation and manual launch smoke for both package types**

- [ ] **Step 4: Commit and push the final optimization batch**

If the target cannot be reached safely, stop and present the report for explicit user exception instead of weakening package compatibility.

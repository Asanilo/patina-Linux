# Linux Bundle-Aware Updater Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Publish a signed Linux updater manifest that routes DEB installations to the DEB and AppImage installations to the AppImage.

**Architecture:** Keep the application updater runtime unchanged and rely on Tauri 2.10.1's existing `{os}-{arch}-{installer}` lookup. Extend release preparation to require both generated signatures and emit `linux-x86_64-deb`, `linux-x86_64-appimage`, and the AppImage-based generic fallback in one static `latest.json`.

**Tech Stack:** Node.js release tooling, Tauri 2 updater artifacts, TypeScript tests executed with Node's strip-types support, GitHub Actions release workflow.

---

## File Map

- Modify `tests/releasePolicy.test.ts`: end-to-end fixtures and assertions for package-specific updater targets and signature failures.
- Modify `scripts/release.ts`: discover paired DEB/AppImage signatures and generate the three-target manifest.
- Modify `README.md`: describe actual Release assets and package-aware update behavior.
- Modify `docs/versioning-and-release-policy.md`: make dual signed artifacts and three manifest targets part of the long-term release contract.
- Reference `docs/superpowers/specs/2026-07-03-linux-bundle-aware-updater-design.md`: approved behavior and safety rationale.

## Task 1: Reproduce Missing DEB Signature Enforcement

**Files:**
- Modify: `tests/releasePolicy.test.ts`
- Test: `tests/releasePolicy.test.ts`

- [ ] **Step 1: Add a failing missing-signature test**

Extract a small fixture helper that creates AppImage, `.AppImage.sig`, and DEB files. Add a test that omits `.deb.sig`, invokes `prepare-linux-release-assets`, and expects rejection containing:

```text
Could not find updater .deb.sig artifact
```

Keep cleanup in `finally` so failed assertions do not leave temporary directories.

- [ ] **Step 2: Run the release test and verify RED**

Run:

```bash
npm run test:release
```

Expected: FAIL because the current release script accepts a DEB without a matching signature.

- [ ] **Step 3: Require the DEB signature in artifact discovery**

In `findLinuxBundles`, locate both signature paths:

```js
const appImageSignatureFilePath = entries.find((entry) =>
  entry.endsWith(".AppImage.sig")
);
const debSignatureFilePath = entries.find((entry) =>
  entry.endsWith(".deb.sig")
);
```

Fail with package-specific messages when either path is absent. Derive each signed artifact path by removing `.sig` and verify that the corresponding file exists. Return explicit AppImage and DEB artifact/signature fields instead of the current generic names.

- [ ] **Step 4: Reject empty DEB signatures**

Read and trim both signature files during release preparation. Fail with the exact signature path when either value is empty.

- [ ] **Step 5: Run the release test and verify GREEN**

Run:

```bash
npm run test:release
```

Expected: PASS after the happy-path fixture is updated to include a non-empty `.deb.sig`.

- [ ] **Step 6: Commit the signature contract**

```bash
git add tests/releasePolicy.test.ts scripts/release.ts
git commit -m "test: require signed Linux package pairs"
```

## Task 2: Emit Bundle-Specific Updater Targets

**Files:**
- Modify: `tests/releasePolicy.test.ts`
- Modify: `scripts/release.ts`
- Test: `tests/releasePolicy.test.ts`

- [ ] **Step 1: Add failing target assertions**

Extend the happy-path fixture with distinct signatures:

```text
appimage-signature
deb-signature
```

Assert all target URLs and signatures:

```js
assert.deepEqual(latest.platforms["linux-x86_64"], {
  url: appImageUrl,
  signature: "appimage-signature",
});
assert.deepEqual(latest.platforms["linux-x86_64-appimage"], {
  url: appImageUrl,
  signature: "appimage-signature",
});
assert.deepEqual(latest.platforms["linux-x86_64-deb"], {
  url: debUrl,
  signature: "deb-signature",
});
```

- [ ] **Step 2: Run the release test and verify RED**

Run:

```bash
npm run test:release
```

Expected: FAIL because the current manifest contains only `linux-x86_64`.

- [ ] **Step 3: Generalize manifest writing**

Change `writeLatestJson` to accept a complete `platforms` object instead of one URL/signature/target tuple. Validate that the object is non-empty before writing it.

Construct fixed URLs from the repository, tag, and normalized release asset names. Pass this platform map:

```js
{
  "linux-x86_64": {
    url: appImageUrl,
    signature: appImageSignature,
  },
  "linux-x86_64-appimage": {
    url: appImageUrl,
    signature: appImageSignature,
  },
  "linux-x86_64-deb": {
    url: debUrl,
    signature: debSignature,
  },
}
```

Do not change the application-side updater target. Tauri's default lookup already checks the package-specific target first.

- [ ] **Step 4: Run the release test and verify GREEN**

Run:

```bash
npm run test:release
```

Expected: PASS with all three targets using the correct artifact and signature.

- [ ] **Step 5: Run release-script static checks**

Run:

```bash
git diff --check
npm run release:validate-changelog -- 1.8.3
```

Expected: both commands exit 0.

- [ ] **Step 6: Commit bundle-aware manifest generation**

```bash
git add tests/releasePolicy.test.ts scripts/release.ts
git commit -m "fix: route Linux updates by package type"
```

## Task 3: Align The Long-Lived Release Contract

**Files:**
- Modify: `README.md`
- Modify: `docs/versioning-and-release-policy.md`
- Test: `tests/releasePolicy.test.ts`

- [ ] **Step 1: Add failing documentation contract assertions**

In the release policy test, assert that the long-lived documentation names:

```text
linux-x86_64-deb
linux-x86_64-appimage
```

Also assert that README no longer claims a released `Patina_<version>_amd64.AppImage.tar.gz` attachment.

- [ ] **Step 2: Run the release test and verify RED**

Run:

```bash
npm run test:release
```

Expected: FAIL because the current docs describe AppImage-only updates and an unpublished tarball.

- [ ] **Step 3: Update README package guidance**

Document that:

- GitHub Releases contain AppImage and DEB, not an AppImage tarball;
- package-aware updater entries route each installed package to its matching signed artifact;
- DEB installation prompts for system authorization during an update;
- the generic `linux-x86_64` target remains AppImage-based for compatibility.

- [ ] **Step 4: Update the release policy**

Replace the AppImage-only updater contract with:

- AppImage and `.AppImage.sig` are required;
- DEB and `.deb.sig` are required;
- `latest.json` must contain the generic, AppImage-specific, and DEB-specific targets;
- package-specific targets must use their matching signatures;
- missing artifacts or signatures fail release preparation.

- [ ] **Step 5: Run the release test and verify GREEN**

Run:

```bash
npm run test:release
```

Expected: PASS.

- [ ] **Step 6: Commit documentation alignment**

```bash
git add README.md docs/versioning-and-release-policy.md tests/releasePolicy.test.ts
git commit -m "docs: define package-aware Linux updates"
```

## Task 4: Full Verification

**Files:**
- Verify only; modify files only if a check exposes a defect in this scope.

- [ ] **Step 1: Validate release metadata**

```bash
npm run release:validate-version-files -- 1.8.3
npm run release:validate-changelog -- 1.8.3
```

Expected: both pass.

- [ ] **Step 2: Run the complete release gate**

```bash
npm run release:check
```

Expected: frontend tests/build, Rust check/test/clippy, GNOME/Chromium extension checks, signed Firefox XPI verification, and changelog validation all pass.

- [ ] **Step 3: Inspect the final diff and repository state**

```bash
git diff --check
git status --short --branch
git log --oneline --decorate -5
```

Expected: no uncommitted implementation changes; commits are scoped to signature enforcement, package routing, and documentation.

- [ ] **Step 4: Do not tag or publish yet**

Report verification results and the next patch-version recommendation. Version bump, changelog entry, push, and tag require a separate release confirmation.

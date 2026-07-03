# GitHub Fork Detachment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create a verifiable offline archive, detach the existing repository from its GitHub fork network, and restore release automation without exposing secrets or changing the repository URL.

**Architecture:** Automate capture and verification of public GitHub state with a repository maintenance script, but keep the irreversible `Leave fork network` confirmation manual. Treat Releases and Actions metadata as disposable remote state backed by checksummed local artifacts; treat secret values as non-exportable and restore them only from verified local sources.

**Tech Stack:** Node.js, TypeScript, GitHub CLI, Git mirror clone, SHA-256, GitHub Actions.

**Reference:** `docs/superpowers/specs/2026-07-04-patina-linux-independent-project-transition-design.md`

---

### Task 1: Build a Safe Detachment Backup Utility

**Files:**
- Create: `scripts/github-detach-backup.ts`
- Create: `tests/githubDetachBackup.test.ts`
- Modify: `package.json`

- [ ] **Step 1: Write failing manifest and safety tests**

Cover: refusal to use a relative or non-empty output directory, mode `0700`, no secret-value fields, SHA-256 mismatch detection, missing release assets, and missing default branch/tag refs.

- [ ] **Step 2: Run the focused test and verify RED**

Run: `node --experimental-strip-types tests/githubDetachBackup.test.ts`

- [ ] **Step 3: Implement pure manifest validation first**

Define typed records for repository metadata, releases/assets, Actions runs, variables, secret names/timestamps, refs, and file hashes. Reject any key matching `/secret.*value|private.*key|password/i`.

- [ ] **Step 4: Implement `capture`, `verify`, and `post-check` commands**

Use `execFile`/`spawn`, never command-string interpolation. `capture` must:

- create a new absolute output directory with mode `0700`;
- `git clone --mirror` the remote;
- call `gh api` for repository/settings/rulesets/branches/releases/runs/variables/secret metadata;
- download release assets and available Actions logs/artifacts;
- hash every downloaded file;
- write a redacted manifest.

Optional resources such as absent branch protection, environments, variables, artifacts, or rulesets may return 404 or an empty list. Record these as explicit empty/unsupported states; do not silently omit an inventory section and do not fail an otherwise complete backup solely because an optional resource does not exist.

`verify` must be offline. `post-check` may read GitHub and compare fork status, refs, tags, workflows, releases, and asset hashes.

- [ ] **Step 5: Add package commands and verify GREEN**

Add `github:detach-backup` and `test:github-detach-backup`, include the test in `check:frontend`, then run both the focused test and `npm run build`.

- [ ] **Step 6: Commit**

```bash
git add scripts/github-detach-backup.ts tests/githubDetachBackup.test.ts package.json
git commit -m "feat: add verified GitHub detachment backup"
```

### Task 2: Capture and Verify the Remote Archive

**Files:**
- External only: `~/backups/patina-linux-github-detach-<timestamp>/`

- [ ] **Step 1: Confirm remote is stable**

Require clean `main`, `main == origin/main`, successful Verify workflow, no in-progress release run, public repo under 1 GB, and no child forks.

- [ ] **Step 2: Confirm secret recovery sources**

Verify the encrypted local Tauri private key and password are available. Compare its public key with all three Tauri updater configs without printing private material.

- [ ] **Step 3: Run capture**

Run the new script with an absolute timestamped backup path outside the repository.

- [ ] **Step 4: Run offline verification**

Require three Releases, eighteen Release assets, all known tags, Actions metadata, two secret names, and valid hashes. Record unavailable expired logs as explicit manifest exceptions.

- [ ] **Step 5: Make a second encrypted copy**

Copy the verified archive to a second durable location. Do not use `/tmp` or a public cloud directory without encryption.

### Task 3: Perform the Manual Fork Detachment

- [ ] **Step 1: Present the final irreversible checklist**

Show backup paths, verification result, remote URL, current fork parent, and GitHub's metadata-loss warning.

- [ ] **Step 2: User performs `Leave fork network`**

The user opens GitHub Settings, Danger Zone, Leave fork network, and confirms `patina-Linux`. The agent does not automate this click or use delete/recreate.

- [ ] **Step 3: Wait until GitHub finishes transitioning**

Do not push, publish, or change settings while GitHub reports the transition in progress.

### Task 4: Restore and Validate the Standalone Repository

- [ ] **Step 1: Run `post-check`**

Require URL `Asanilo/patina-Linux`, `fork=false`, matching default branch, refs, tags, and workflow files.

- [ ] **Step 2: Rebuild missing Releases**

Recreate only missing releases from archived tag/title/body/prerelease state and upload assets. Verify hashes after upload. Accept that reconstructed publication timestamps differ; preserve originals in archive JSON.

- [ ] **Step 3: Restore updater Secrets securely**

Use `gh secret set` with each value supplied through protected stdin from the encrypted local source. Do not place values in argv, logs, shell history, docs, or the backup manifest.

- [ ] **Step 4: Restore variables and repository settings**

Apply only settings captured in the manifest. Do not mutate issue state or create new branches/PRs.

- [ ] **Step 5: Trigger and inspect Verify**

Run `gh workflow run verify.yml`, wait for completion, and require success.

- [ ] **Step 6: Confirm updater continuity**

Fetch the existing `latest.json`, verify its signature fields and repository URLs, and confirm no endpoint changed.

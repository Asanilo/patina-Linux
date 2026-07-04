import assert from "node:assert/strict";
import { mkdir, mkdtemp, readFile, rm, stat, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  assertManifestHasNoSecretValues,
  ensureCaptureDirectory,
  hashFile,
  type BackupManifest,
  verifyBackup,
} from "../scripts/github-detach-backup.ts";

const roots: string[] = [];

async function tempRoot() {
  const root = await mkdtemp(path.join(tmpdir(), "patina-github-detach-test-"));
  roots.push(root);
  return root;
}

async function runTest(name: string, fn: () => Promise<void> | void) {
  try {
    await fn();
    console.log(`PASS ${name}`);
  } catch (error) {
    console.error(`FAIL ${name}`);
    throw error;
  }
}

function fixtureManifest(): BackupManifest {
  return {
    schema_version: 1,
    captured_at: "2026-07-04T00:00:00.000Z",
    repository: {
      name_with_owner: "Asanilo/patina-Linux",
      html_url: "https://github.com/Asanilo/patina-Linux",
      default_branch: "main",
      fork: true,
      parent: "Ceceliaee/patina",
    },
    refs: [
      { name: "refs/heads/main", oid: "a".repeat(40) },
      { name: "refs/tags/v1.8.3", oid: "b".repeat(40) },
    ],
    releases: [{
      id: 1,
      tag_name: "v1.8.3",
      name: "Patina Linux v1.8.3",
      body: "Release",
      draft: false,
      prerelease: false,
      published_at: "2026-07-04T00:00:00.000Z",
      assets: [{
        id: 2,
        name: "Patina_1.8.3_amd64.deb",
        size: 3,
        download_count: 0,
        updated_at: "2026-07-04T00:00:00.000Z",
        path: "releases/v1.8.3/Patina_1.8.3_amd64.deb",
      }],
    }],
    actions: { runs: [], artifacts: [], log_exceptions: [] },
    settings: {
      branches: [],
      branch_protection: { state: "empty", items: [] },
      rulesets: { state: "empty", items: [] },
      environments: { state: "empty", items: [] },
      variables: { state: "empty", items: [] },
    },
    secrets: [{ name: "TAURI_SIGNING_PRIVATE_KEY", created_at: null, updated_at: null }],
    files: [],
    exceptions: [],
  };
}

async function writeValidFixture(root: string) {
  await mkdir(path.join(root, "releases", "v1.8.3"), { recursive: true });
  await mkdir(path.join(root, "repository.git", "refs", "heads"), { recursive: true });
  await mkdir(path.join(root, "repository.git", "refs", "tags"), { recursive: true });
  await writeFile(path.join(root, "releases", "v1.8.3", "Patina_1.8.3_amd64.deb"), "deb");
  await writeFile(path.join(root, "repository.git", "refs", "heads", "main"), `${"a".repeat(40)}\n`);
  await writeFile(path.join(root, "repository.git", "refs", "tags", "v1.8.3"), `${"b".repeat(40)}\n`);

  const manifest = fixtureManifest();
  for (const relativePath of [
    "releases/v1.8.3/Patina_1.8.3_amd64.deb",
    "repository.git/refs/heads/main",
    "repository.git/refs/tags/v1.8.3",
  ]) {
    manifest.files.push(await hashFile(root, relativePath));
  }
  await writeFile(path.join(root, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  return manifest;
}

await runTest("capture directory rejects relative paths", async () => {
  await assert.rejects(() => ensureCaptureDirectory("relative/backup"), /absolute/);
});

await runTest("capture directory rejects non-empty paths", async () => {
  const root = await tempRoot();
  await writeFile(path.join(root, "existing.txt"), "occupied");
  await assert.rejects(() => ensureCaptureDirectory(root), /empty/);
});

await runTest("capture directory is owner-only", async () => {
  const parent = await tempRoot();
  const root = path.join(parent, "backup");
  await ensureCaptureDirectory(root);
  const mode = (await stat(root)).mode & 0o777;
  assert.equal(mode, 0o700);
});

await runTest("manifest rejects secret values and passwords", () => {
  assert.throws(
    () => assertManifestHasNoSecretValues({ secrets: [{ name: "TOKEN", secret_value: "no" }] }),
    /secret_value/,
  );
  assert.throws(
    () => assertManifestHasNoSecretValues({ recovery: { password: "no" } }),
    /password/,
  );
  assert.throws(
    () => assertManifestHasNoSecretValues({ recovery: { private_key: "no" } }),
    /private_key/,
  );
});

await runTest("offline verification detects SHA-256 mismatch", async () => {
  const root = await tempRoot();
  await writeValidFixture(root);
  await writeFile(path.join(root, "releases", "v1.8.3", "Patina_1.8.3_amd64.deb"), "changed");
  await assert.rejects(() => verifyBackup(root), /SHA-256 mismatch/);
});

await runTest("offline verification detects missing release assets", async () => {
  const root = await tempRoot();
  await writeValidFixture(root);
  await rm(path.join(root, "releases", "v1.8.3", "Patina_1.8.3_amd64.deb"));
  await assert.rejects(() => verifyBackup(root), /release asset is missing/);
});

await runTest("offline verification requires default branch and release tag refs", async () => {
  const root = await tempRoot();
  const manifest = await writeValidFixture(root);
  manifest.refs = [];
  await writeFile(path.join(root, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  await assert.rejects(() => verifyBackup(root), /default branch ref/);
});

await runTest("offline verification detects a missing release tag ref", async () => {
  const root = await tempRoot();
  const manifest = await writeValidFixture(root);
  manifest.refs = manifest.refs.filter((ref) => ref.name !== "refs/tags/v1.8.3");
  await writeFile(path.join(root, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  await assert.rejects(() => verifyBackup(root), /release tag ref/);
});

for (const root of roots) {
  await rm(root, { recursive: true, force: true });
}

console.log("Validated GitHub detachment backup safety and offline verification");

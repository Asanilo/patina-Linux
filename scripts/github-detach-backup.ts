import { createHash } from "node:crypto";
import {
  chmod,
  lstat,
  mkdir,
  mkdtemp,
  opendir,
  readFile,
  readdir,
  rm,
  stat,
  writeFile,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { fileURLToPath } from "node:url";

type JsonRecord = Record<string, unknown>;

export type BackupFile = {
  path: string;
  size: number;
  sha256: string;
};

export type BackupRef = {
  name: string;
  oid: string;
};

export type BackupReleaseAsset = {
  id: number;
  name: string;
  size: number;
  download_count: number;
  updated_at: string | null;
  path: string;
};

export type BackupRelease = {
  id: number;
  tag_name: string;
  name: string | null;
  body: string | null;
  draft: boolean;
  prerelease: boolean;
  published_at: string | null;
  assets: BackupReleaseAsset[];
};

export type OptionalInventory = {
  state: "available" | "empty" | "unsupported";
  items: unknown[];
  note?: string;
};

export type BackupManifest = {
  schema_version: 1;
  captured_at: string;
  repository: {
    name_with_owner: string;
    html_url: string;
    default_branch: string;
    fork: boolean;
    parent: string | null;
  };
  refs: BackupRef[];
  releases: BackupRelease[];
  actions: {
    runs: JsonRecord[];
    artifacts: JsonRecord[];
    log_exceptions: Array<{ run_id: number; reason: string }>;
  };
  settings: {
    branches: unknown[];
    branch_protection: OptionalInventory;
    rulesets: OptionalInventory;
    environments: OptionalInventory;
    variables: OptionalInventory;
    workflows?: OptionalInventory;
  };
  secrets: Array<{
    name: string;
    created_at: string | null;
    updated_at: string | null;
  }>;
  files: BackupFile[];
  exceptions: string[];
};

class CommandError extends Error {
  readonly command: string;
  readonly args: string[];
  readonly exitCode: number | null;
  readonly stderr: string;

  constructor(command: string, args: string[], exitCode: number | null, stderr: string) {
    super(`${command} exited with ${exitCode ?? "unknown"}: ${stderr.trim()}`);
    this.name = "CommandError";
    this.command = command;
    this.args = args;
    this.exitCode = exitCode;
    this.stderr = stderr;
  }
}

type CommandResult = {
  stdout: Buffer;
  stderr: Buffer;
};

function runCommand(command: string, args: string[], cwd?: string): Promise<CommandResult> {
  return new Promise((resolve, reject) => {
    const child = spawn(command, args, {
      cwd,
      env: process.env,
      shell: false,
      stdio: ["ignore", "pipe", "pipe"],
    });
    const stdout: Buffer[] = [];
    const stderr: Buffer[] = [];

    child.stdout.on("data", (chunk: Buffer) => stdout.push(chunk));
    child.stderr.on("data", (chunk: Buffer) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (code) => {
      const result = { stdout: Buffer.concat(stdout), stderr: Buffer.concat(stderr) };
      if (code === 0) {
        resolve(result);
        return;
      }
      reject(new CommandError(command, args, code, result.stderr.toString("utf8")));
    });
  });
}

function parseJson<T>(buffer: Buffer, label: string): T {
  try {
    return JSON.parse(buffer.toString("utf8")) as T;
  } catch (error) {
    throw new Error(`${label} returned invalid JSON: ${String(error)}`);
  }
}

async function ghJson<T>(endpoint: string): Promise<T> {
  const result = await runCommand("gh", ["api", endpoint]);
  return parseJson<T>(result.stdout, `gh api ${endpoint}`);
}

async function ghPaginated(endpoint: string): Promise<JsonRecord[]> {
  const result = await runCommand("gh", ["api", "--paginate", "--slurp", endpoint]);
  const pages = parseJson<unknown[]>(result.stdout, `gh api ${endpoint}`);
  return pages.flatMap((page) => Array.isArray(page) ? page as JsonRecord[] : [page as JsonRecord]);
}

async function ghPaginatedField(endpoint: string, field: string): Promise<JsonRecord[]> {
  const pages = await ghPaginated(endpoint);
  return pages.flatMap((page) => {
    const value = page[field];
    return Array.isArray(value) ? value as JsonRecord[] : [];
  });
}

function optionalInventory(items: unknown[], note?: string): OptionalInventory {
  return {
    state: items.length > 0 ? "available" : "empty",
    items,
    ...(note ? { note } : {}),
  };
}

async function captureOptional(
  label: string,
  loader: () => Promise<unknown[]>,
  exceptions: string[],
): Promise<OptionalInventory> {
  try {
    return optionalInventory(await loader());
  } catch (error) {
    const detail = error instanceof CommandError ? error.stderr.trim() : String(error);
    exceptions.push(`${label}: ${detail || "unavailable"}`);
    return { state: "unsupported", items: [], note: detail || "unavailable" };
  }
}

function requiredString(record: JsonRecord, key: string, label: string): string {
  const value = record[key];
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label}.${key} is missing`);
  }
  return value;
}

function optionalString(value: unknown): string | null {
  return typeof value === "string" ? value : null;
}

function requiredNumber(record: JsonRecord, key: string, label: string): number {
  const value = record[key];
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error(`${label}.${key} is missing`);
  }
  return value;
}

function safeRelativePath(root: string, relativePath: string): string {
  if (path.isAbsolute(relativePath)) {
    throw new Error(`backup path must be relative: ${relativePath}`);
  }
  const normalized = path.normalize(relativePath);
  if (normalized === ".." || normalized.startsWith(`..${path.sep}`)) {
    throw new Error(`backup path escapes its root: ${relativePath}`);
  }
  return path.join(root, normalized);
}

function safeSegment(value: string): string {
  return encodeURIComponent(value).replaceAll("%", "_");
}

async function writeJson(root: string, relativePath: string, value: unknown) {
  const outputPath = safeRelativePath(root, relativePath);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, `${JSON.stringify(value, null, 2)}\n`, { mode: 0o600 });
}

async function writeBinary(root: string, relativePath: string, value: Buffer) {
  const outputPath = safeRelativePath(root, relativePath);
  await mkdir(path.dirname(outputPath), { recursive: true });
  await writeFile(outputPath, value, { mode: 0o600 });
}

export async function ensureCaptureDirectory(outputDir: string): Promise<string> {
  if (!path.isAbsolute(outputDir)) {
    throw new Error("backup output directory must be absolute");
  }

  try {
    const info = await stat(outputDir);
    if (!info.isDirectory()) {
      throw new Error("backup output path must be a directory");
    }
    if ((await readdir(outputDir)).length > 0) {
      throw new Error("backup output directory must be empty");
    }
    await chmod(outputDir, 0o700);
  } catch (error) {
    const code = (error as NodeJS.ErrnoException).code;
    if (code !== "ENOENT") {
      throw error;
    }
    await mkdir(outputDir, { recursive: true, mode: 0o700 });
    await chmod(outputDir, 0o700);
  }
  return outputDir;
}

export function assertManifestHasNoSecretValues(value: unknown, pathParts: string[] = []): void {
  if (Array.isArray(value)) {
    value.forEach((item, index) => assertManifestHasNoSecretValues(item, [...pathParts, String(index)]));
    return;
  }
  if (value === null || typeof value !== "object") {
    return;
  }
  for (const [key, child] of Object.entries(value as JsonRecord)) {
    const currentPath = [...pathParts, key];
    if (/secret.*value|private.*key|password/i.test(key)) {
      throw new Error(`backup manifest contains forbidden field ${currentPath.join(".")}`);
    }
    assertManifestHasNoSecretValues(child, currentPath);
  }
}

export async function hashFile(root: string, relativePath: string): Promise<BackupFile> {
  const absolutePath = safeRelativePath(root, relativePath);
  const fileInfo = await stat(absolutePath);
  if (!fileInfo.isFile()) {
    throw new Error(`backup file is not regular: ${relativePath}`);
  }
  const content = await readFile(absolutePath);
  return {
    path: relativePath.split(path.sep).join("/"),
    size: content.byteLength,
    sha256: createHash("sha256").update(content).digest("hex"),
  };
}

async function listRegularFiles(root: string, relativeDir = ""): Promise<string[]> {
  const absoluteDir = safeRelativePath(root, relativeDir);
  const directory = await opendir(absoluteDir);
  const files: string[] = [];
  for await (const entry of directory) {
    const relativePath = path.join(relativeDir, entry.name);
    const absolutePath = safeRelativePath(root, relativePath);
    const info = await lstat(absolutePath);
    if (info.isSymbolicLink()) {
      throw new Error(`backup contains unsupported symlink: ${relativePath}`);
    }
    if (info.isDirectory()) {
      files.push(...await listRegularFiles(root, relativePath));
    } else if (info.isFile() && relativePath !== "manifest.json") {
      files.push(relativePath);
    }
  }
  return files.sort();
}

function parseRefs(output: string): BackupRef[] {
  return output
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const separator = line.indexOf(" ");
      if (separator < 1) {
        throw new Error(`invalid git ref line: ${line}`);
      }
      return { oid: line.slice(0, separator), name: line.slice(separator + 1) };
    })
    .filter((ref) => !ref.name.endsWith("^{}"))
    .sort((left, right) => left.name.localeCompare(right.name));
}

async function mirrorRefs(mirrorDir: string): Promise<BackupRef[]> {
  try {
    const result = await runCommand("git", ["--git-dir", mirrorDir, "show-ref"]);
    return parseRefs(result.stdout.toString("utf8"));
  } catch {
    const refsRoot = path.join(mirrorDir, "refs");
    const refs: BackupRef[] = [];
    async function walk(relativeDir: string) {
      const absoluteDir = path.join(refsRoot, relativeDir);
      let entries;
      try {
        entries = await readdir(absoluteDir, { withFileTypes: true });
      } catch {
        return;
      }
      for (const entry of entries) {
        const relativePath = path.join(relativeDir, entry.name);
        if (entry.isDirectory()) {
          await walk(relativePath);
        } else if (entry.isFile()) {
          const oid = (await readFile(path.join(refsRoot, relativePath), "utf8")).trim();
          refs.push({ name: `refs/${relativePath.split(path.sep).join("/")}`, oid });
        }
      }
    }
    await walk("");
    return refs.sort((left, right) => left.name.localeCompare(right.name));
  }
}

function validateManifestShape(manifest: BackupManifest) {
  if (manifest.schema_version !== 1) {
    throw new Error(`unsupported backup schema version: ${String(manifest.schema_version)}`);
  }
  if (!manifest.repository?.name_with_owner || !manifest.repository.default_branch) {
    throw new Error("backup manifest repository metadata is incomplete");
  }
  const names = new Set(manifest.files.map((file) => file.path));
  if (names.size !== manifest.files.length) {
    throw new Error("backup manifest contains duplicate file records");
  }
}

export async function verifyBackup(outputDir: string): Promise<BackupManifest> {
  if (!path.isAbsolute(outputDir)) {
    throw new Error("backup directory must be absolute");
  }
  const directoryInfo = await stat(outputDir);
  if ((directoryInfo.mode & 0o077) !== 0) {
    throw new Error("backup directory permissions must be 0700 or stricter");
  }
  const manifest = JSON.parse(await readFile(path.join(outputDir, "manifest.json"), "utf8")) as BackupManifest;
  assertManifestHasNoSecretValues(manifest);
  validateManifestShape(manifest);

  const defaultRef = `refs/heads/${manifest.repository.default_branch}`;
  if (!manifest.refs.some((ref) => ref.name === defaultRef)) {
    throw new Error(`backup manifest is missing default branch ref ${defaultRef}`);
  }
  for (const release of manifest.releases) {
    const tagRef = `refs/tags/${release.tag_name}`;
    if (!manifest.refs.some((ref) => ref.name === tagRef)) {
      throw new Error(`backup manifest is missing release tag ref ${tagRef}`);
    }
    for (const asset of release.assets) {
      try {
        const info = await stat(safeRelativePath(outputDir, asset.path));
        if (!info.isFile()) {
          throw new Error("not a file");
        }
      } catch {
        throw new Error(`release asset is missing: ${asset.path}`);
      }
      if (!manifest.files.some((file) => file.path === asset.path)) {
        throw new Error(`release asset has no hash record: ${asset.path}`);
      }
    }
  }

  for (const expected of manifest.files) {
    let actual: BackupFile;
    try {
      actual = await hashFile(outputDir, expected.path);
    } catch {
      throw new Error(`backup file is missing: ${expected.path}`);
    }
    if (actual.size !== expected.size || actual.sha256 !== expected.sha256) {
      throw new Error(`SHA-256 mismatch for ${expected.path}`);
    }
  }

  const actualFiles = await listRegularFiles(outputDir);
  const recordedFiles = manifest.files.map((file) => file.path).sort();
  if (actualFiles.join("\n") !== recordedFiles.join("\n")) {
    throw new Error("backup contains unrecorded or missing files");
  }

  const capturedRefs = new Map(manifest.refs.map((ref) => [ref.name, ref.oid]));
  const actualRefs = await mirrorRefs(path.join(outputDir, "repository.git"));
  for (const refName of [
    defaultRef,
    ...manifest.releases.map((release) => `refs/tags/${release.tag_name}`),
  ]) {
    const actual = actualRefs.find((ref) => ref.name === refName);
    if (!actual || actual.oid !== capturedRefs.get(refName)) {
      throw new Error(`mirror ref mismatch for ${refName}`);
    }
  }

  return manifest;
}

function mapRelease(raw: JsonRecord): BackupRelease {
  const tagName = requiredString(raw, "tag_name", "release");
  const assets = Array.isArray(raw.assets) ? raw.assets as JsonRecord[] : [];
  return {
    id: requiredNumber(raw, "id", `release ${tagName}`),
    tag_name: tagName,
    name: optionalString(raw.name),
    body: optionalString(raw.body),
    draft: raw.draft === true,
    prerelease: raw.prerelease === true,
    published_at: optionalString(raw.published_at),
    assets: assets.map((asset) => {
      const name = requiredString(asset, "name", `release ${tagName} asset`);
      return {
        id: requiredNumber(asset, "id", `release ${tagName} asset ${name}`),
        name,
        size: requiredNumber(asset, "size", `release ${tagName} asset ${name}`),
        download_count: typeof asset.download_count === "number" ? asset.download_count : 0,
        updated_at: optionalString(asset.updated_at),
        path: `releases/${safeSegment(tagName)}/${safeSegment(name)}`,
      };
    }),
  };
}

function mapActionRun(raw: JsonRecord): JsonRecord {
  return {
    id: raw.id,
    name: raw.name,
    event: raw.event,
    status: raw.status,
    conclusion: raw.conclusion,
    head_branch: raw.head_branch,
    head_sha: raw.head_sha,
    created_at: raw.created_at,
    updated_at: raw.updated_at,
    html_url: raw.html_url,
  };
}

function mapArtifact(raw: JsonRecord, archivePath: string | null): JsonRecord {
  return {
    id: raw.id,
    name: raw.name,
    size_in_bytes: raw.size_in_bytes,
    expired: raw.expired,
    created_at: raw.created_at,
    expires_at: raw.expires_at,
    updated_at: raw.updated_at,
    workflow_run: raw.workflow_run,
    archive_path: archivePath,
  };
}

export async function captureBackup(outputDir: string, repo = "Asanilo/patina-Linux") {
  const root = await ensureCaptureDirectory(outputDir);
  const exceptions: string[] = [];
  const repository = await ghJson<JsonRecord>(`repos/${repo}`);
  const fullName = requiredString(repository, "full_name", "repository");
  const defaultBranch = requiredString(repository, "default_branch", "repository");
  const cloneUrl = requiredString(repository, "clone_url", "repository");
  const parent = repository.parent && typeof repository.parent === "object"
    ? optionalString((repository.parent as JsonRecord).full_name)
    : null;

  await writeJson(root, "metadata/repository.json", repository);
  await runCommand("git", ["clone", "--mirror", cloneUrl, path.join(root, "repository.git")]);
  const refs = await mirrorRefs(path.join(root, "repository.git"));
  await writeJson(root, "metadata/refs.json", refs);

  const rawReleases = await ghPaginated(`repos/${repo}/releases?per_page=100`);
  const releases = rawReleases.map(mapRelease);
  await writeJson(root, "metadata/releases.json", rawReleases);
  for (const release of releases) {
    for (const asset of release.assets) {
      const result = await runCommand("gh", [
        "api",
        "-H",
        "Accept: application/octet-stream",
        `repos/${repo}/releases/assets/${asset.id}`,
      ]);
      await writeBinary(root, asset.path, result.stdout);
      if (result.stdout.byteLength !== asset.size) {
        throw new Error(`downloaded release asset size mismatch for ${asset.name}`);
      }
    }
  }

  const branches = await ghPaginated(`repos/${repo}/branches?per_page=100`);
  await writeJson(root, "metadata/branches.json", branches);
  const branchProtection = await captureOptional(
    "branch protection",
    async () => [await ghJson(`repos/${repo}/branches/${encodeURIComponent(defaultBranch)}/protection`)],
    exceptions,
  );
  const rulesets = await captureOptional(
    "rulesets",
    () => ghPaginated(`repos/${repo}/rulesets?per_page=100`),
    exceptions,
  );
  const environments = await captureOptional(
    "environments",
    async () => {
      const response = await ghJson<JsonRecord>(`repos/${repo}/environments?per_page=100`);
      return Array.isArray(response.environments) ? response.environments as JsonRecord[] : [];
    },
    exceptions,
  );
  const variables = await captureOptional(
    "actions variables",
    async () => {
      const response = await ghJson<JsonRecord>(`repos/${repo}/actions/variables?per_page=100`);
      return Array.isArray(response.variables) ? response.variables as JsonRecord[] : [];
    },
    exceptions,
  );
  const workflows = await captureOptional(
    "actions workflows",
    async () => {
      const response = await ghJson<JsonRecord>(`repos/${repo}/actions/workflows?per_page=100`);
      return Array.isArray(response.workflows) ? response.workflows as JsonRecord[] : [];
    },
    exceptions,
  );
  await writeJson(root, "metadata/settings.json", {
    branches,
    branch_protection: branchProtection,
    rulesets,
    environments,
    variables,
    workflows,
  });

  const secretResponse = await ghJson<JsonRecord>(`repos/${repo}/actions/secrets?per_page=100`);
  const rawSecrets = Array.isArray(secretResponse.secrets) ? secretResponse.secrets as JsonRecord[] : [];
  const secrets = rawSecrets.map((secret) => ({
    name: requiredString(secret, "name", "actions secret"),
    created_at: optionalString(secret.created_at),
    updated_at: optionalString(secret.updated_at),
  }));
  await writeJson(root, "metadata/secret-names.json", secrets);

  const rawRuns = await ghPaginatedField(`repos/${repo}/actions/runs?per_page=100`, "workflow_runs");
  const runs = rawRuns.map(mapActionRun);
  await writeJson(root, "metadata/actions-runs.json", runs);
  const rawArtifacts = await ghPaginatedField(`repos/${repo}/actions/artifacts?per_page=100`, "artifacts");
  const artifacts: JsonRecord[] = [];
  for (const artifact of rawArtifacts) {
    const id = requiredNumber(artifact, "id", "actions artifact");
    const name = requiredString(artifact, "name", `actions artifact ${id}`);
    const expired = artifact.expired === true;
    let archivePath = expired ? null : `actions/artifacts/${id}-${safeSegment(name)}.zip`;
    if (archivePath) {
      try {
        const result = await runCommand("gh", [
          "api",
          "-H",
          "Accept: application/vnd.github+json",
          `repos/${repo}/actions/artifacts/${id}/zip`,
        ]);
        await writeBinary(root, archivePath, result.stdout);
      } catch (error) {
        exceptions.push(`artifact ${id}: ${error instanceof Error ? error.message : String(error)}`);
        archivePath = null;
      }
    }
    artifacts.push(mapArtifact(artifact, archivePath));
  }
  await writeJson(root, "metadata/actions-artifacts.json", artifacts);

  const logExceptions: Array<{ run_id: number; reason: string }> = [];
  for (const run of runs) {
    const runId = typeof run.id === "number" ? run.id : null;
    if (runId === null) {
      continue;
    }
    try {
      const result = await runCommand("gh", [
        "api",
        "-H",
        "Accept: application/vnd.github+json",
        `repos/${repo}/actions/runs/${runId}/logs`,
      ]);
      await writeBinary(root, `actions/logs/${runId}.zip`, result.stdout);
    } catch (error) {
      const reason = error instanceof CommandError ? error.stderr.trim() : String(error);
      logExceptions.push({ run_id: runId, reason: reason || "log unavailable" });
    }
  }
  await writeJson(root, "metadata/actions-log-exceptions.json", logExceptions);

  const manifest: BackupManifest = {
    schema_version: 1,
    captured_at: new Date().toISOString(),
    repository: {
      name_with_owner: fullName,
      html_url: requiredString(repository, "html_url", "repository"),
      default_branch: defaultBranch,
      fork: repository.fork === true,
      parent,
    },
    refs,
    releases,
    actions: { runs, artifacts, log_exceptions: logExceptions },
    settings: {
      branches,
      branch_protection: branchProtection,
      rulesets,
      environments,
      variables,
      workflows,
    },
    secrets,
    files: [],
    exceptions,
  };
  assertManifestHasNoSecretValues(manifest);
  const files = await listRegularFiles(root);
  manifest.files = await Promise.all(files.map((file) => hashFile(root, file)));
  await writeJson(root, "manifest.json", manifest);
  await verifyBackup(root);
  console.log(`Captured and verified GitHub backup at ${root}`);
}

function sha256Buffer(value: Buffer): string {
  return createHash("sha256").update(value).digest("hex");
}

export async function postCheckBackup(outputDir: string, repo?: string) {
  const manifest = await verifyBackup(outputDir);
  const targetRepo = repo ?? manifest.repository.name_with_owner;
  const repository = await ghJson<JsonRecord>(`repos/${targetRepo}`);
  if (repository.fork === true) {
    throw new Error(`${targetRepo} is still attached to a fork network`);
  }
  if (repository.default_branch !== manifest.repository.default_branch) {
    throw new Error("remote default branch changed after detachment");
  }

  const remoteUrl = requiredString(repository, "clone_url", "repository");
  const remoteRefsResult = await runCommand("git", ["ls-remote", "--refs", remoteUrl]);
  const remoteRefs = new Map(parseRefs(remoteRefsResult.stdout.toString("utf8")).map((ref) => [ref.name, ref.oid]));
  for (const ref of manifest.refs) {
    if ((ref.name.startsWith("refs/heads/") || ref.name.startsWith("refs/tags/"))
      && remoteRefs.get(ref.name) !== ref.oid) {
      throw new Error(`remote ref mismatch after detachment: ${ref.name}`);
    }
  }

  const remoteReleases = (await ghPaginated(`repos/${targetRepo}/releases?per_page=100`)).map(mapRelease);
  for (const release of manifest.releases) {
    const remoteRelease = remoteReleases.find((item) => item.tag_name === release.tag_name);
    if (!remoteRelease) {
      throw new Error(`remote release is missing after detachment: ${release.tag_name}`);
    }
    if (remoteRelease.name !== release.name
      || remoteRelease.body !== release.body
      || remoteRelease.draft !== release.draft
      || remoteRelease.prerelease !== release.prerelease) {
      throw new Error(`remote release metadata changed: ${release.tag_name}`);
    }
    for (const asset of release.assets) {
      const remoteAsset = remoteRelease.assets.find((item) => item.name === asset.name);
      if (!remoteAsset) {
        throw new Error(`remote release asset is missing: ${release.tag_name}/${asset.name}`);
      }
      const expected = manifest.files.find((file) => file.path === asset.path);
      if (!expected) {
        throw new Error(`archived release asset hash is missing: ${asset.path}`);
      }
      const result = await runCommand("gh", [
        "api",
        "-H",
        "Accept: application/octet-stream",
        `repos/${targetRepo}/releases/assets/${remoteAsset.id}`,
      ]);
      if (result.stdout.byteLength !== expected.size || sha256Buffer(result.stdout) !== expected.sha256) {
        throw new Error(`remote release asset hash mismatch: ${release.tag_name}/${asset.name}`);
      }
    }
  }
  console.log(`Post-detachment check passed for ${targetRepo}`);
}

function usage(): never {
  throw new Error([
    "Usage:",
    "  github-detach-backup.ts capture <absolute-output-dir> [owner/repo]",
    "  github-detach-backup.ts verify <absolute-output-dir>",
    "  github-detach-backup.ts post-check <absolute-output-dir> [owner/repo]",
  ].join("\n"));
}

async function main() {
  const [command, outputDir, repo] = process.argv.slice(2);
  if (!command || !outputDir) {
    usage();
  }
  if (command === "capture") {
    await captureBackup(outputDir, repo);
    return;
  }
  if (command === "verify") {
    await verifyBackup(outputDir);
    console.log(`Offline verification passed for ${outputDir}`);
    return;
  }
  if (command === "post-check") {
    await postCheckBackup(outputDir, repo);
    return;
  }
  usage();
}

const scriptPath = process.argv[1] ? path.resolve(process.argv[1]) : "";
if (scriptPath === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}

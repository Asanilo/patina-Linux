import { execFile } from "node:child_process";
import { lstat, open, readFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const MAX_JSON_FILE_BYTES = 64 * 1024;
const MAX_COMMAND_OUTPUT_BYTES = 256 * 1024;
const INSTALLED_PATHS = {
  desktop: "/usr/bin/Patina",
  daemon: "/usr/bin/patinad",
  unit: "/usr/lib/systemd/user/patinad.service",
};
const ACCEPTANCE_PHASES = new Set([
  "baseline",
  "installed",
  "managed",
  "rolled-back",
  "uninstalled",
]);

function bounded(value: string): string {
  return value.slice(0, MAX_COMMAND_OUTPUT_BYTES).trim();
}

async function run(command: string, args: string[]) {
  try {
    const result = await execFileAsync(command, args, {
      encoding: "utf8",
      maxBuffer: MAX_COMMAND_OUTPUT_BYTES,
      timeout: 5_000,
    });
    return { ok: true, stdout: bounded(result.stdout), stderr: bounded(result.stderr) };
  } catch (error) {
    const failure = error as NodeJS.ErrnoException & { stdout?: string; stderr?: string };
    return {
      ok: false,
      stdout: bounded(failure.stdout ?? ""),
      stderr: bounded(failure.stderr ?? failure.message ?? String(error)),
    };
  }
}

export function parseSystemdProperties(raw: string): Record<string, string> {
  return Object.fromEntries(
    raw
      .split(/\r?\n/)
      .filter(Boolean)
      .map((line) => {
        const separator = line.indexOf("=");
        return separator === -1
          ? [line, ""]
          : [line.slice(0, separator), line.slice(separator + 1)];
      }),
  );
}

function modeString(mode: number): string {
  return (mode & 0o777).toString(8).padStart(3, "0");
}

async function inspectPath(filePath: string) {
  try {
    const metadata = await lstat(filePath);
    return {
      exists: true,
      regular: metadata.isFile() && !metadata.isSymbolicLink(),
      directory: metadata.isDirectory() && !metadata.isSymbolicLink(),
      symlink: metadata.isSymbolicLink(),
      mode: modeString(metadata.mode),
      uid: metadata.uid,
      size: metadata.size,
      modifiedAt: metadata.mtime.toISOString(),
    };
  } catch (error) {
    const code = error && typeof error === "object" && "code" in error
      ? String(error.code)
      : "UNKNOWN";
    return { exists: false, error: code };
  }
}

async function readBoundedJson(filePath: string): Promise<unknown | null> {
  const metadata = await lstat(filePath);
  if (!metadata.isFile() || metadata.isSymbolicLink()) {
    throw new Error("not a regular file");
  }
  if (metadata.size > MAX_JSON_FILE_BYTES) {
    throw new Error(`file exceeds ${MAX_JSON_FILE_BYTES} bytes`);
  }
  return JSON.parse(await readFile(filePath, "utf8"));
}

async function optionalJsonSummary(
  filePath: string,
  select: (value: Record<string, unknown>) => Record<string, unknown>,
) {
  try {
    const value = await readBoundedJson(filePath);
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      throw new Error("expected a JSON object");
    }
    return { present: true, value: select(value as Record<string, unknown>) };
  } catch (error) {
    const code = error && typeof error === "object" && "code" in error
      ? String(error.code)
      : "";
    if (code === "ENOENT") {
      return { present: false };
    }
    return {
      present: true,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export function summarizeCapabilities(payload: unknown) {
  const envelope = payload && typeof payload === "object" && !Array.isArray(payload)
    ? payload as Record<string, unknown>
    : {};
  const data = envelope.data && typeof envelope.data === "object" && !Array.isArray(envelope.data)
    ? envelope.data as Record<string, unknown>
    : {};
  const capability = (name: string) => {
    const value = data[name];
    if (!value || typeof value !== "object" || Array.isArray(value)) {
      return { owned: false, ready: false };
    }
    const record = value as Record<string, unknown>;
    return { owned: record.owned === true, ready: record.ready === true };
  };
  const protocol = data.protocol && typeof data.protocol === "object" && !Array.isArray(data.protocol)
    ? data.protocol as Record<string, unknown>
    : {};
  const writeApi = data.write_api && typeof data.write_api === "object" && !Array.isArray(data.write_api)
    ? data.write_api as Record<string, unknown>
    : {};

  return {
    serverVersion: typeof data.server_version === "string" ? data.server_version : null,
    runtimeHost: typeof data.runtime_host === "string" ? data.runtime_host : null,
    protocol: {
      current: typeof protocol.current === "number" ? protocol.current : null,
      minSupportedClient: typeof protocol.min_supported_client === "number"
        ? protocol.min_supported_client
        : null,
      maxSupportedClient: typeof protocol.max_supported_client === "number"
        ? protocol.max_supported_client
        : null,
    },
    tracking: capability("tracking"),
    browserActivityBridge: capability("browser_activity_bridge"),
    tools: capability("tools"),
    daemonService: capability("daemon_service"),
    writeApi: {
      available: writeApi.available === true,
      operationCount: Array.isArray(writeApi.operations) ? writeApi.operations.length : 0,
    },
  };
}

export function summarizeCutoverReservation(value: Record<string, unknown>) {
  return {
    version: value.version,
    state: value.status,
    profile: value.profile,
    requestId: value.request_id,
    failureCode: value.failure_code,
    backgroundTrackingAtLogin: value.background_tracking_at_login,
    desktopLaunchAtLogin: value.desktop_launch_at_login,
  };
}

function check(id: string, passed: boolean, detail: string) {
  return { id, status: passed ? "pass" : "fail", detail };
}

export function evaluateAcceptanceEvidence(evidence: any) {
  const phase = evidence.phase;
  const packageInstalled = evidence.package?.installed === true;
  const installedFilesPresent = ["desktop", "daemon", "unit"].every((name) => {
    const entry = evidence.installedFiles?.[name];
    return entry?.exists === true && entry?.regular === true;
  });
  const databaseHealthy = evidence.database?.quickCheck === "ok";
  const tokenSecure = evidence.apiToken?.exists === true
    && evidence.apiToken?.regular === true
    && evidence.apiToken?.mode === "600"
    && evidence.apiToken?.uid === evidence.host?.uid;
  const serviceStopped = !evidence.systemd?.error
    && evidence.systemd?.ActiveState === "inactive"
    && evidence.systemd?.ExecMainPID === "0";
  const checks = [];

  if (phase === "uninstalled") {
    checks.push(
      check("package-removed", !packageInstalled, "the patina package is not installed"),
      check("installed-files-removed", !["desktop", "daemon", "unit"].some(
        (name) => evidence.installedFiles?.[name]?.exists === true,
      ), "package-owned executables and unit are absent"),
      check("data-retained", evidence.database?.exists === true, "the user database still exists"),
      check("database-integrity", databaseHealthy, "the retained database passes quick_check"),
      check("api-token-retained", tokenSecure, "the API token remains an owner-only regular file"),
      check("service-stopped", serviceStopped, "systemd confirms the service is inactive with no main PID"),
    );
    return checks;
  }

  if (phase === "baseline") {
    checks.push(
      check("package-installed", packageInstalled, "the existing patina package is installed"),
      check("database-integrity", databaseHealthy, "the existing database passes quick_check"),
      check("api-token-permissions", tokenSecure, "the API token is a regular owner-only 0600 file"),
    );
    if (evidence.expectedVersion) {
      checks.push(check(
        "package-version",
        evidence.package?.version === evidence.expectedVersion,
        `installed ${evidence.package?.version ?? "missing"}; expected ${evidence.expectedVersion}`,
      ));
    }
    return checks;
  }

  checks.push(
    check("package-installed", packageInstalled, "the patina package is installed"),
    check("installed-files", installedFilesPresent, "Desktop, patinad, and the user unit are regular files"),
    check("systemd-unit-loaded", evidence.systemd?.LoadState === "loaded", "systemd loaded the packaged user unit"),
    check("database-integrity", databaseHealthy, "the active database passes quick_check"),
    check("api-token-permissions", tokenSecure, "the API token is a regular owner-only 0600 file"),
  );

  if (evidence.expectedVersion) {
    checks.push(check(
      "package-version",
      evidence.package?.version === evidence.expectedVersion,
      `installed ${evidence.package?.version ?? "missing"}; expected ${evidence.expectedVersion}`,
    ));
  }

  if (phase === "managed") {
    const capabilities = evidence.api?.capabilities;
    checks.push(
      check("service-active", evidence.systemd?.ActiveState === "active", "patinad.service is active"),
      check("cutover-completed", evidence.cutover?.value?.state === "completed", "owner cutover is completed"),
      check("daemon-lease", evidence.runtimeLease?.value?.role === "daemon", "the runtime lease reports daemon ownership"),
      check("daemon-service-pid", Number(evidence.systemd?.ExecMainPID) > 0
        && Number(evidence.systemd?.ExecMainPID) === evidence.runtimeLease?.value?.pid,
      "the runtime lease PID matches the active service main PID"),
      check("daemon-api", evidence.api?.reachable === true, "the authenticated daemon API is reachable"),
      check("daemon-runtime-host", capabilities?.runtimeHost === "daemon", "capabilities identify the daemon runtime"),
      check("tracking-ready", capabilities?.tracking?.owned === true && capabilities?.tracking?.ready === true, "tracking is owned and ready"),
      check("service-capability", capabilities?.daemonService?.owned === true && capabilities?.daemonService?.ready === true, "the managed service capability is ready"),
    );
    if (evidence.expectedVersion) {
      checks.push(check("daemon-version", capabilities?.serverVersion === evidence.expectedVersion,
        `running daemon ${capabilities?.serverVersion ?? "unknown"}; expected ${evidence.expectedVersion}`));
    }
  } else if (phase === "rolled-back") {
    checks.push(
      check("service-stopped", serviceStopped, "systemd confirms the service is inactive with no main PID"),
      check("cutover-rolled-back", evidence.cutover?.value?.state === "rolled_back", "owner cutover is committed as rolled-back"),
      check("daemon-lease-released", evidence.runtimeLease?.value?.role !== "daemon", "the daemon no longer owns the runtime lease"),
    );
  }

  return checks;
}

function parseArguments(args: string[]) {
  const options: { phase: string; expectedVersion?: string; output?: string } = {
    phase: "installed",
  };
  for (let index = 0; index < args.length; index += 1) {
    const argument = args[index];
    if (argument === "--phase") {
      options.phase = args[++index] ?? "";
    } else if (argument === "--expected-version") {
      options.expectedVersion = args[++index];
    } else if (argument === "--output") {
      options.output = args[++index];
    } else {
      throw new Error(`unknown argument: ${argument}`);
    }
  }
  if (!ACCEPTANCE_PHASES.has(options.phase)) {
    throw new Error(`invalid --phase; expected one of ${[...ACCEPTANCE_PHASES].join(", ")}`);
  }
  if (options.output && !path.isAbsolute(options.output)) {
    throw new Error("--output must be an absolute path");
  }
  return options;
}

function parsePackage(raw: string) {
  const [name, version, architecture, ...status] = raw.split("\t");
  return {
    installed: name === "patina" && status.join("\t") === "install ok installed",
    name: name || null,
    version: version || null,
    architecture: architecture || null,
    status: status.join("\t") || null,
  };
}

async function inspectDatabase(dbPath: string) {
  const metadata = await inspectPath(dbPath);
  if (!metadata.exists || !metadata.regular) {
    return { ...metadata, quickCheck: null, counts: null };
  }
  const quickCheck = await run("sqlite3", [
    "-readonly",
    "-cmd",
    ".timeout 2000",
    dbPath,
    "PRAGMA quick_check;",
  ]);
  const counts = await run("sqlite3", [
    "-readonly",
    "-cmd",
    ".timeout 2000",
    "-separator",
    "\t",
    dbPath,
    [
      "SELECT",
      "(SELECT COUNT(*) FROM sessions),",
      "(SELECT COUNT(*) FROM sessions WHERE end_time IS NULL),",
      "(SELECT COUNT(*) FROM web_activity_segments),",
      "(SELECT COUNT(*) FROM web_activity_segments WHERE end_time IS NULL),",
      "COALESCE((SELECT MAX(COALESCE(end_time, start_time)) FROM sessions), 0);",
    ].join(" "),
  ]);
  const [sessions, activeSessions, webSegments, activeWebSegments, latestSessionBoundary] =
    counts.ok ? counts.stdout.split("\t").map(Number) : [];
  return {
    ...metadata,
    quickCheck: quickCheck.ok ? quickCheck.stdout : null,
    quickCheckError: quickCheck.ok ? null : quickCheck.stderr,
    counts: counts.ok
      ? { sessions, activeSessions, webSegments, activeWebSegments, latestSessionBoundary }
      : null,
    countsError: counts.ok ? null : counts.stderr,
  };
}

async function discoverApiPort(dbPath: string): Promise<number> {
  const result = await run("sqlite3", [
    "-readonly",
    dbPath,
    "SELECT value FROM settings WHERE key = 'local_api_port' LIMIT 1;",
  ]);
  if (!result.ok || !result.stdout) {
    return 14840;
  }
  let value: unknown = result.stdout;
  try {
    value = JSON.parse(result.stdout);
  } catch {
    // Older databases may store the port as an unquoted scalar.
  }
  const port = Number(value);
  return Number.isInteger(port) && port >= 1024 && port <= 65535 ? port : 14840;
}

async function inspectApi(port: number, tokenPath: string, tokenMetadata: any) {
  if (
    !tokenMetadata.exists
    || !tokenMetadata.regular
    || tokenMetadata.mode !== "600"
    || tokenMetadata.uid !== (process.getuid?.() ?? null)
  ) {
    return { reachable: false, port, error: "secure API token file is unavailable" };
  }
  try {
    const token = (await readFile(tokenPath, "utf8")).trim();
    if (!token) {
      return { reachable: false, port, error: "API token file is empty" };
    }
    const response = await fetch(`http://127.0.0.1:${port}/api/v1/capabilities`, {
      headers: { Authorization: `Bearer ${token}` },
      signal: AbortSignal.timeout(2_000),
    });
    const body = await response.text();
    if (body.length > MAX_COMMAND_OUTPUT_BYTES) {
      throw new Error("capability response is too large");
    }
    if (!response.ok) {
      return { reachable: false, port, status: response.status, error: "capability request failed" };
    }
    return {
      reachable: true,
      port,
      status: response.status,
      capabilities: summarizeCapabilities(JSON.parse(body)),
    };
  } catch (error) {
    return {
      reachable: false,
      port,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

async function collectEvidence(options: ReturnType<typeof parseArguments>) {
  const home = process.env.HOME;
  if (!home) {
    throw new Error("HOME is unavailable");
  }
  const configRoot = process.env.XDG_CONFIG_HOME || path.join(home, ".config");
  const dataBase = process.env.XDG_DATA_HOME || path.join(home, ".local", "share");
  const controlRoot = path.join(configRoot, "Patina");
  const stableDataRoot = path.join(dataBase, "Patina");
  const dataAnchor = await optionalJsonSummary(
    path.join(controlRoot, "data-anchor.json"),
    (value) => ({ format: value.format, profile: value.profile, dataRoot: value.dataRoot }),
  );
  const anchoredDataRoot = dataAnchor.present
    && !dataAnchor.error
    && dataAnchor.value?.profile === "production"
    && typeof dataAnchor.value?.dataRoot === "string"
    ? dataAnchor.value.dataRoot
    : null;
  const dataRoot = anchoredDataRoot ?? stableDataRoot;
  const dbPath = path.join(dataRoot, "patina.db");
  const tokenPath = path.join(stableDataRoot, "api_token");
  const packageResult = await run("dpkg-query", [
    "-W",
    "-f=${Package}\t${Version}\t${Architecture}\t${Status}",
    "patina",
  ]);
  const systemdResult = await run("systemctl", [
    "--user",
    "show",
    "patinad.service",
    "--no-pager",
    "--property=LoadState,UnitFileState,ActiveState,SubState,FragmentPath,ExecMainPID,ExecMainStatus,NRestarts",
  ]);
  const apiToken = await inspectPath(tokenPath);
  const database = await inspectDatabase(dbPath);
  const apiPort = await discoverApiPort(dbPath);
  const evidence: any = {
    format: "patina.patinad-installed-acceptance.v1",
    capturedAt: new Date().toISOString(),
    phase: options.phase,
    expectedVersion: options.expectedVersion ?? null,
    host: { platform: process.platform, architecture: process.arch, uid: process.getuid?.() ?? null },
    package: packageResult.ok
      ? parsePackage(packageResult.stdout)
      : { installed: false, error: packageResult.stderr },
    installedFiles: Object.fromEntries(await Promise.all(
      Object.entries(INSTALLED_PATHS).map(async ([name, filePath]) => [name, await inspectPath(filePath)]),
    )),
    systemd: systemdResult.ok
      ? parseSystemdProperties(systemdResult.stdout)
      : { error: systemdResult.stderr },
    paths: { controlRoot, stableDataRoot, dataRoot, dbPath, tokenPath },
    dataAnchor,
    pendingStorageMigration: await optionalJsonSummary(
      path.join(controlRoot, "storage-migration-pending.json"),
      (value) => ({ format: value.format, id: value.id, profile: value.profile, state: value.state }),
    ),
    cutover: await optionalJsonSummary(
      path.join(controlRoot, "runtime-owner-cutover.json"),
      summarizeCutoverReservation,
    ),
    runtimeLease: await optionalJsonSummary(
      path.join(controlRoot, "runtime-owner.lock"),
      (value) => ({ role: value.role, pid: value.pid, profile: value.profile, acquiredAtMs: value.acquiredAtMs }),
    ),
    apiToken,
    database,
    api: await inspectApi(apiPort, tokenPath, apiToken),
  };
  evidence.checks = evaluateAcceptanceEvidence(evidence);
  evidence.passed = evidence.checks.every((entry: any) => entry.status === "pass");
  return evidence;
}

export async function writeEvidence(filePath: string, content: string) {
  const handle = await open(filePath, "wx", 0o600);
  try {
    await handle.writeFile(content, "utf8");
    await handle.sync();
  } finally {
    await handle.close();
  }
}

async function main() {
  try {
    const options = parseArguments(process.argv.slice(2));
    const evidence = await collectEvidence(options);
    const serialized = `${JSON.stringify(evidence, null, 2)}\n`;
    if (options.output) {
      await writeEvidence(options.output, serialized);
      console.log(`Acceptance evidence written to ${options.output}`);
    } else {
      process.stdout.write(serialized);
    }
    if (!evidence.passed) {
      process.exitCode = 1;
    }
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}

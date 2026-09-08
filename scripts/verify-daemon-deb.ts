import { execFile } from "node:child_process";
import {
  lstat,
  mkdtemp,
  readFile,
  readdir,
  rm,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import process from "node:process";
import { promisify } from "node:util";
import { fileURLToPath } from "node:url";

const execFileAsync = promisify(execFile);
const GNOME_EXTENSION_UUID = "patina-window-tracker@patina";
const REQUIRED_FILES = [
  "usr/bin/Patina",
  "usr/bin/patinad",
  "usr/lib/systemd/user/patinad.service",
  `usr/share/gnome-shell/extensions/${GNOME_EXTENSION_UUID}/extension.js`,
  `usr/share/gnome-shell/extensions/${GNOME_EXTENSION_UUID}/metadata.json`,
] as const;
const FORBIDDEN_ENABLE_PATHS = [
  "etc/systemd/user/default.target.wants/patinad.service",
  "usr/lib/systemd/user/default.target.wants/patinad.service",
] as const;

export interface DebianPackageMetadata {
  architecture: string;
  packageName: string;
  version: string;
}

async function readDebField(debPath: string, field: string): Promise<string> {
  const { stdout } = await execFileAsync("dpkg-deb", ["--field", debPath, field], {
    encoding: "utf8",
    maxBuffer: 64 * 1024,
  });
  return stdout.trim();
}

export function validateDaemonDebMetadata(
  metadata: DebianPackageMetadata,
  expectedVersion: string,
): string[] {
  const errors: string[] = [];
  if (metadata.packageName !== "patina") {
    errors.push(`package name is ${metadata.packageName || "missing"}, expected patina`);
  }
  if (metadata.version !== expectedVersion) {
    errors.push(`package version is ${metadata.version || "missing"}, expected ${expectedVersion}`);
  }
  if (metadata.architecture !== "amd64") {
    errors.push(`package architecture is ${metadata.architecture || "missing"}, expected amd64`);
  }
  return errors;
}

async function validateRegularFile(
  root: string,
  relativePath: string,
  executable: boolean,
): Promise<string[]> {
  const absolutePath = path.join(root, relativePath);
  try {
    const metadata = await lstat(absolutePath);
    if (!metadata.isFile() || metadata.isSymbolicLink()) {
      return [`${relativePath} must be a regular non-symlink file`];
    }
    if (metadata.size === 0) {
      return [`${relativePath} must not be empty`];
    }
    if (executable && (metadata.mode & 0o111) === 0) {
      return [`${relativePath} must be executable`];
    }
    if (!executable && (metadata.mode & 0o022) !== 0) {
      return [`${relativePath} must not be group- or world-writable`];
    }
    return [];
  } catch (error) {
    return [`${relativePath} is missing: ${String(error)}`];
  }
}

function requiredUnitLines(): string[] {
  return [
    "ExecStart=/usr/bin/patinad --profile production --serve-api --track",
    "Environment=PATINA_SYSTEMD_SERVICE=patinad.service",
    "Restart=on-failure",
    "KillSignal=SIGINT",
    "UMask=0077",
    "NoNewPrivileges=true",
    "ProtectSystem=strict",
    "WantedBy=default.target",
  ];
}

export async function validateExtractedDaemonDeb(root: string): Promise<string[]> {
  const errors = (
    await Promise.all(REQUIRED_FILES.map((relativePath) => validateRegularFile(
      root,
      relativePath,
      relativePath === "usr/bin/Patina" || relativePath === "usr/bin/patinad",
    )))
  ).flat();

  for (const relativePath of FORBIDDEN_ENABLE_PATHS) {
    try {
      await lstat(path.join(root, relativePath));
      errors.push(`${relativePath} must not be packaged; first-launch migration owns service enablement`);
    } catch (error) {
      const code = error && typeof error === "object" && "code" in error
        ? String(error.code)
        : "";
      if (code !== "ENOENT") {
        errors.push(`could not inspect forbidden enable path ${relativePath}: ${String(error)}`);
      }
    }
  }

  const unitPath = path.join(root, "usr/lib/systemd/user/patinad.service");
  try {
    const unit = await readFile(unitPath, "utf8");
    const unitLines = unit.split(/\r?\n/).map((line) => line.trim());
    for (const line of requiredUnitLines()) {
      if (!unitLines.includes(line)) {
        errors.push(`patinad.service is missing required line: ${line}`);
      }
    }
    const execDirectives = unitLines.filter((line) => /^Exec(?:Start|Stop|Reload)/.test(line));
    if (execDirectives.length !== 1 || execDirectives[0] !== requiredUnitLines()[0]) {
      errors.push("patinad.service must contain only the expected ExecStart directive");
    }
    if (/\bsystemctl\b|\benable\s+--now\b/.test(unit)) {
      errors.push("patinad.service must not recursively enable or start itself");
    }
  } catch (error) {
    errors.push(`could not read patinad.service: ${String(error)}`);
  }

  const metadataPath = path.join(
    root,
    "usr/share/gnome-shell/extensions",
    GNOME_EXTENSION_UUID,
    "metadata.json",
  );
  try {
    const metadata = JSON.parse(await readFile(metadataPath, "utf8"));
    if (metadata.uuid !== GNOME_EXTENSION_UUID) {
      errors.push(`GNOME extension UUID is ${String(metadata.uuid)}, expected ${GNOME_EXTENSION_UUID}`);
    }
  } catch (error) {
    errors.push(`could not parse GNOME extension metadata: ${String(error)}`);
  }

  return errors;
}

async function validateMaintainerScripts(controlRoot: string): Promise<string[]> {
  const errors: string[] = [];
  const entries = await readdir(controlRoot, { withFileTypes: true });
  for (const entry of entries) {
    if (!entry.isFile() || !["preinst", "postinst", "prerm", "postrm"].includes(entry.name)) {
      continue;
    }
    const content = await readFile(path.join(controlRoot, entry.name), "utf8");
    if (/patinad\.service|systemctl[^\n]*(?:enable|start)/i.test(content)) {
      errors.push(`${entry.name} must not enable or start patinad.service`);
    }
  }
  return errors;
}

export async function verifyDaemonDeb(debPath: string, expectedVersion: string): Promise<void> {
  const resolvedDebPath = path.resolve(debPath);
  const debMetadata = await lstat(resolvedDebPath);
  if (!debMetadata.isFile() || debMetadata.isSymbolicLink()) {
    throw new Error("DEB path must be a regular non-symlink file");
  }

  const metadata = {
    packageName: await readDebField(resolvedDebPath, "Package"),
    version: await readDebField(resolvedDebPath, "Version"),
    architecture: await readDebField(resolvedDebPath, "Architecture"),
  };
  const errors = validateDaemonDebMetadata(metadata, expectedVersion);
  const tempRoot = await mkdtemp(path.join(tmpdir(), "patina-daemon-deb-verify-"));
  try {
    const payloadRoot = path.join(tempRoot, "payload");
    const controlRoot = path.join(tempRoot, "control");
    await execFileAsync("dpkg-deb", ["--extract", resolvedDebPath, payloadRoot], {
      maxBuffer: 1024 * 1024,
    });
    await execFileAsync("dpkg-deb", ["--control", resolvedDebPath, controlRoot], {
      maxBuffer: 1024 * 1024,
    });
    errors.push(...await validateExtractedDaemonDeb(payloadRoot));
    errors.push(...await validateMaintainerScripts(controlRoot));
  } finally {
    await rm(tempRoot, { recursive: true, force: true });
  }

  if (errors.length > 0) {
    throw new Error(`daemon-backed DEB verification failed:\n- ${errors.join("\n- ")}`);
  }
}

async function main() {
  const [debPath, expectedVersion] = process.argv.slice(2);
  if (!debPath || !expectedVersion) {
    console.error("Usage: verify-daemon-deb <deb-path> <expected-version>");
    process.exitCode = 1;
    return;
  }
  try {
    await verifyDaemonDeb(debPath, expectedVersion);
    console.log(`Verified daemon-backed DEB: ${path.resolve(debPath)}`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}

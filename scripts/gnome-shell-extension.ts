import { cp, mkdir, readFile, rm, stat } from "node:fs/promises";
import { dirname, join, relative } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const EXTENSION_UUID = "patina-window-tracker@patina";
const SOURCE_DIR = join(REPO_ROOT, "extensions", "gnome-shell", EXTENSION_UUID);
const BUILD_DIR = join(REPO_ROOT, "dist", "extensions", "gnome-shell", EXTENSION_UUID);
const REQUIRED_FILES = ["metadata.json", "extension.js"] as const;

type GnomeExtensionMetadata = {
  uuid?: string;
  name?: string;
  description?: string;
  version?: number;
  "shell-version"?: string[];
};

export function gnomeShellExtensionInstallDir(env = process.env) {
  const xdgDataHome = typeof env.xdgDataHome === "string" ? env.xdgDataHome : env.XDG_DATA_HOME;
  const home = typeof env.home === "string" ? env.home : env.HOME;
  const dataHome = xdgDataHome?.trim()
    ? xdgDataHome
    : join(home?.trim() || ".", ".local", "share");

  return join(dataHome, "gnome-shell", "extensions", EXTENSION_UUID);
}

export function validateGnomeShellExtensionSourceText(
  metadataText: string,
  extensionJs: string,
) {
  const errors: string[] = [];
  let metadata: GnomeExtensionMetadata | null = null;

  try {
    metadata = JSON.parse(metadataText) as GnomeExtensionMetadata;
  } catch (error) {
    return [`GNOME Shell extension check failed. metadata.json is invalid JSON: ${String(error)}`];
  }

  if (metadata.uuid !== EXTENSION_UUID) {
    errors.push(`GNOME Shell extension check failed. metadata uuid must be ${EXTENSION_UUID}.`);
  }
  if (!metadata.name?.trim()) {
    errors.push("GNOME Shell extension check failed. metadata name is required.");
  }
  if (!metadata.description?.trim()) {
    errors.push("GNOME Shell extension check failed. metadata description is required.");
  }
  if (!Number.isInteger(metadata.version) || (metadata.version ?? 0) < 1) {
    errors.push("GNOME Shell extension check failed. metadata version must be a positive integer.");
  }
  if (!Array.isArray(metadata["shell-version"]) || metadata["shell-version"].length === 0) {
    errors.push("GNOME Shell extension check failed. metadata shell-version must not be empty.");
  }
  if (!extensionJs.includes("org.patina.WindowTracker")) {
    errors.push("GNOME Shell extension check failed. extension.js must define org.patina.WindowTracker.");
  }
  if (!extensionJs.includes("GetFocusedWindow")) {
    errors.push("GNOME Shell extension check failed. extension.js must export GetFocusedWindow.");
  }
  if (!extensionJs.includes("FocusedWindowChanged")) {
    errors.push("GNOME Shell extension check failed. extension.js must emit FocusedWindowChanged.");
  }

  return errors;
}

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

async function ensureFile(relativePath: string) {
  const filePath = join(SOURCE_DIR, relativePath);
  try {
    const fileStat = await stat(filePath);
    if (!fileStat.isFile()) {
      fail(`GNOME Shell extension check failed. Expected file: ${relativePath}`);
    }
  } catch {
    fail(`GNOME Shell extension check failed. Missing file: ${relativePath}`);
  }
}

async function checkExtension() {
  for (const file of REQUIRED_FILES) {
    await ensureFile(file);
  }

  const errors = validateGnomeShellExtensionSourceText(
    await readFile(join(SOURCE_DIR, "metadata.json"), "utf8"),
    await readFile(join(SOURCE_DIR, "extension.js"), "utf8"),
  );
  if (errors.length > 0) {
    fail(errors.join("\n"));
  }

  console.log("GNOME Shell extension check passed.");
}

async function copyExtension(outputDir: string) {
  await rm(outputDir, { force: true, recursive: true });
  await mkdir(outputDir, { recursive: true });
  for (const file of REQUIRED_FILES) {
    await cp(join(SOURCE_DIR, file), join(outputDir, file));
  }
}

async function buildExtension() {
  await checkExtension();
  await copyExtension(BUILD_DIR);
  console.log(`GNOME Shell extension build written to ${relative(REPO_ROOT, BUILD_DIR)}.`);
}

async function installExtension() {
  await checkExtension();
  const installDir = gnomeShellExtensionInstallDir();
  await copyExtension(installDir);
  console.log(`GNOME Shell extension installed to ${installDir}.`);
  console.log("Run `gnome-extensions enable patina-window-tracker@patina` and log out/in if GNOME Shell has cached an older copy.");
}

async function main() {
  const command = process.argv[2];
  switch (command) {
    case "check":
      await checkExtension();
      break;
    case "build":
      await buildExtension();
      break;
    case "install":
      await installExtension();
      break;
    default:
      fail("Usage: node --experimental-strip-types scripts/gnome-shell-extension.ts <check|build|install>");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main();
}

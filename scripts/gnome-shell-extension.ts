import { execFile } from "node:child_process";
import { cp, mkdir, readFile, rm, stat } from "node:fs/promises";
import { dirname, join, relative } from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { promisify } from "node:util";

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const EXTENSION_UUID = "patina-window-tracker@patina";
const SOURCE_DIR = join(REPO_ROOT, "extensions", "gnome-shell", EXTENSION_UUID);
const BUILD_DIR = join(REPO_ROOT, "dist", "extensions", "gnome-shell", EXTENSION_UUID);
const ESM_SOURCE_DIR = join(SOURCE_DIR, "esm");
const ESM_BUILD_DIR = join(REPO_ROOT, "dist", "extensions", "gnome-shell-esm", EXTENSION_UUID);
const REQUIRED_FILES = ["metadata.json", "extension.js"] as const;
const execFileAsync = promisify(execFile);
type ExtensionVariant = "legacy" | "esm";

function sourceDir(variant: ExtensionVariant) {
  return variant === "esm" ? ESM_SOURCE_DIR : SOURCE_DIR;
}

function buildDir(variant: ExtensionVariant) {
  return variant === "esm" ? ESM_BUILD_DIR : BUILD_DIR;
}

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

export function supportsGnomeShellVersion(metadataText: string, versionOutput: string): boolean {
  const match = versionOutput.match(/^GNOME Shell (\d+)(?:\.|$)/m);
  if (!match) return false;
  const metadata = JSON.parse(metadataText) as GnomeExtensionMetadata;
  return Array.isArray(metadata["shell-version"]) &&
    metadata["shell-version"].includes(match[1]);
}

export function validateGnomeShellExtensionSourceText(
  metadataText: string,
  extensionJs: string,
  variant: ExtensionVariant = "legacy",
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
  } else if (metadata["shell-version"].some((version) => {
    const major = Number(version);
    return !Number.isInteger(major) || (variant === "esm" ? major < 45 : major >= 45);
  })) {
    errors.push(`GNOME Shell extension check failed. ${variant} shell versions are incompatible with this entry point.`);
  }
  if (variant === "esm") {
    if (!extensionJs.includes("from 'resource:///org/gnome/shell/extensions/extension.js'") ||
        !extensionJs.includes("export default class") ||
        !extensionJs.includes("extends Extension")) {
      errors.push("GNOME Shell extension check failed. ESM extension.js must export an Extension subclass.");
    }
  } else if (extensionJs.includes("export default") || extensionJs.includes("from 'gi://")) {
    errors.push("GNOME Shell extension check failed. Legacy extension.js must not use ESM syntax.");
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

  if (!extensionJs.includes("org.patina.WindowTracker1") || !extensionJs.includes("GetSnapshot")) {
    errors.push("GNOME Shell extension check failed. extension.js must export WindowTracker1.GetSnapshot.");
  }

  return errors;
}

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

async function ensureFile(relativePath: string, variant: ExtensionVariant) {
  const filePath = join(sourceDir(variant), relativePath);
  try {
    const fileStat = await stat(filePath);
    if (!fileStat.isFile()) {
      fail(`GNOME Shell extension check failed. Expected file: ${relativePath}`);
    }
  } catch {
    fail(`GNOME Shell extension check failed. Missing file: ${relativePath}`);
  }
}

async function checkExtension(variant: ExtensionVariant) {
  for (const file of REQUIRED_FILES) {
    await ensureFile(file, variant);
  }

  const errors = validateGnomeShellExtensionSourceText(
    await readFile(join(sourceDir(variant), "metadata.json"), "utf8"),
    await readFile(join(sourceDir(variant), "extension.js"), "utf8"),
    variant,
  );
  if (errors.length > 0) {
    fail(errors.join("\n"));
  }

  console.log(`GNOME Shell ${variant} extension check passed.`);
}

async function copyExtension(outputDir: string, variant: ExtensionVariant) {
  await rm(outputDir, { force: true, recursive: true });
  await mkdir(outputDir, { recursive: true });
  for (const file of REQUIRED_FILES) {
    await cp(join(sourceDir(variant), file), join(outputDir, file));
  }
}

async function buildExtension(variant: ExtensionVariant) {
  await checkExtension(variant);
  const outputDir = buildDir(variant);
  await copyExtension(outputDir, variant);
  console.log(`GNOME Shell ${variant} extension build written to ${relative(REPO_ROOT, outputDir)}.`);
}

async function installExtension(variant: ExtensionVariant) {
  await checkExtension(variant);
  const metadata = await readFile(join(sourceDir(variant), "metadata.json"), "utf8");
  let shellVersion: string;
  try {
    shellVersion = (await execFileAsync("gnome-shell", ["--version"])).stdout.trim();
  } catch (error) {
    fail(`GNOME Shell ${variant} extension install requires an available Shell: ${String(error)}`);
  }
  if (!supportsGnomeShellVersion(metadata, shellVersion)) {
    fail(`GNOME Shell ${variant} extension does not declare support for ${shellVersion}.`);
  }
  const installDir = gnomeShellExtensionInstallDir();
  await copyExtension(installDir, variant);
  console.log(`GNOME Shell ${variant} extension installed to ${installDir}.`);
  console.log("Run `gnome-extensions enable patina-window-tracker@patina` and log out/in if GNOME Shell has cached an older copy.");
}

async function main() {
  const command = process.argv[2];
  switch (command) {
    case "check":
      await checkExtension("legacy");
      break;
    case "build":
      await buildExtension("legacy");
      break;
    case "install":
      await installExtension("legacy");
      break;
    case "check-esm":
      await checkExtension("esm");
      break;
    case "build-esm":
      await buildExtension("esm");
      break;
    case "install-esm":
      await installExtension("esm");
      break;
    default:
      fail("Usage: node --experimental-strip-types scripts/gnome-shell-extension.ts <check|build|install|check-esm|build-esm|install-esm>");
  }
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  await main();
}

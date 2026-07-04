import { readFile, stat } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SOURCE_DIR = join(REPO_ROOT, "extensions", "firefox");
const SIGNED_XPI_PATH = join(SOURCE_DIR, "dist", "patina-web-sync.xpi");
const REQUIRED_ICON_FILES = {
  "32": "icons/icon-32.png",
  "64": "icons/icon-64.png",
  "128": "icons/icon-128.png",
} as const;
const REQUIRED_FILES = [
  "manifest.json",
  "background.js",
  "options.html",
  "options.js",
  "popup.html",
  "popup.js",
  "README.en.md",
  "README.zh-CN.md",
  "package.sh",
  ...Object.values(REQUIRED_ICON_FILES),
] as const;

type FirefoxManifest = {
  manifest_version?: number;
  name?: string;
  version?: string;
  background?: {
    scripts?: string[];
  };
  permissions?: string[];
  icons?: Record<string, string>;
  options_ui?: {
    page?: string;
  };
  browser_action?: {
    default_popup?: string;
    default_icon?: Record<string, string>;
  };
  browser_specific_settings?: {
    gecko?: {
      id?: string;
    };
  };
};

function fail(message: string): never {
  console.error(message);
  process.exit(1);
}

function runUnzip(args: string[], failureMessage: string): string {
  try {
    return execFileSync("unzip", args, { encoding: "utf8" });
  } catch (error) {
    const result = error as { status?: number | null; stdout?: string | Buffer };
    if (result.status === 0 && result.stdout) {
      return result.stdout.toString();
    }
    fail(failureMessage);
  }
}

async function ensureFile(relativePath: string) {
  try {
    const fileStat = await stat(join(SOURCE_DIR, relativePath));
    if (!fileStat.isFile()) {
      fail(`Firefox extension check failed. Expected file: ${relativePath}`);
    }
  } catch {
    fail(`Firefox extension check failed. Missing file: ${relativePath}`);
  }
}

async function readManifest() {
  let raw = "";
  try {
    raw = await readFile(join(SOURCE_DIR, "manifest.json"), "utf8");
  } catch {
    fail("Firefox extension check failed. Missing extensions/firefox/manifest.json.");
  }

  try {
    return JSON.parse(raw) as FirefoxManifest;
  } catch (error) {
    fail(`Firefox extension check failed. manifest.json is invalid JSON: ${String(error)}`);
  }
}

async function checkExtension() {
  for (const file of REQUIRED_FILES) {
    await ensureFile(file);
  }

  const manifest = await readManifest();
  const background = await readFile(join(SOURCE_DIR, "background.js"), "utf8");
  const options = await readFile(join(SOURCE_DIR, "options.js"), "utf8");
  const popup = await readFile(join(SOURCE_DIR, "popup.js"), "utf8");
  const permissions = new Set(manifest.permissions ?? []);

  if (manifest.manifest_version !== 2) {
    fail("Firefox extension check failed. manifest_version must be 2 for Zen/Firefox compatibility.");
  }
  if (!manifest.name?.trim() || !manifest.version?.trim()) {
    fail("Firefox extension check failed. manifest name and version are required.");
  }
  if (!manifest.browser_specific_settings?.gecko?.id?.trim()) {
    fail("Firefox extension check failed. gecko extension id is required.");
  }
  if (!manifest.background?.scripts?.includes("background.js")) {
    fail("Firefox extension check failed. background.scripts must include background.js.");
  }
  if (manifest.options_ui?.page !== "options.html") {
    fail("Firefox extension check failed. options_ui.page must be options.html.");
  }
  if (manifest.browser_action?.default_popup !== "popup.html") {
    fail("Firefox extension check failed. browser_action.default_popup must be popup.html.");
  }
  for (const permission of ["alarms", "storage", "tabs", "http://127.0.0.1/*", "http://localhost/*"]) {
    if (!permissions.has(permission)) {
      fail(`Firefox extension check failed. Missing permission: ${permission}.`);
    }
  }
  if (background.includes("/_favicon/") || background.includes("chromeCachedFaviconUrl(")) {
    fail("Firefox extension check failed. Firefox build must not use Chromium's /_favicon/ cache.");
  }
  if (!background.startsWith("const chrome = browser;")) {
    fail("Firefox extension check failed. background.js must use the browser API compatibility alias.");
  }
  if (!options.startsWith("const chrome = browser;") || !popup.startsWith("const chrome = browser;")) {
    fail("Firefox extension check failed. options.js and popup.js must use the browser API compatibility alias.");
  }
  if (!background.includes("chrome.runtime.getBrowserInfo")) {
    fail("Firefox extension check failed. Browser identity must prefer runtime.getBrowserInfo().");
  }
  if (!background.includes("await browserKind()")) {
    fail("Firefox extension check failed. Active-tab payload must await browser identity.");
  }
  const genericFirefoxIndex = background.indexOf('identity.includes("firefox")');
  if (genericFirefoxIndex < 0) {
    fail("Firefox extension check failed. Generic Firefox identity detection is required.");
  }
  for (const fork of ["zen", "floorp", "iceweasel", "librewolf"]) {
    const forkIndex = background.indexOf(`identity.includes("${fork}")`);
    if (forkIndex < 0 || forkIndex > genericFirefoxIndex) {
      fail(`Firefox extension check failed. ${fork} identity must be detected before generic Firefox.`);
    }
  }

  console.log("Firefox extension check passed.");
}

function readSignedXpiEntry(entry: string): string {
  return runUnzip(
    ["-p", SIGNED_XPI_PATH, entry],
    `Firefox signed extension check failed. Cannot read ${entry} from the signed XPI.`,
  );
}

async function verifySignedExtension() {
  await ensureFile("dist/patina-web-sync.xpi");
  const entries = runUnzip(
    ["-Z1", SIGNED_XPI_PATH],
    "Firefox signed extension check failed. The signed XPI is not a readable ZIP archive.",
  );
  if (!entries.includes("META-INF/cose.sig") && !entries.includes("META-INF/mozilla.rsa")) {
    fail("Firefox signed extension check failed. Mozilla signature metadata is missing.");
  }

  const sourceManifest = await readManifest();
  let signedManifest: FirefoxManifest;
  try {
    signedManifest = JSON.parse(readSignedXpiEntry("manifest.json")) as FirefoxManifest;
  } catch (error) {
    fail(`Firefox signed extension check failed. Signed manifest is invalid: ${String(error)}`);
  }
  if (signedManifest.version !== sourceManifest.version) {
    fail(
      `Firefox signed extension check failed. Signed version ${signedManifest.version ?? "missing"} does not match source ${sourceManifest.version ?? "missing"}.`,
    );
  }

  const sourceBackground = await readFile(join(SOURCE_DIR, "background.js"), "utf8");
  const signedBackground = readSignedXpiEntry("background.js");
  if (signedBackground.replace(/\r\n/g, "\n") !== sourceBackground.replace(/\r\n/g, "\n")) {
    fail("Firefox signed extension check failed. Signed background.js does not match the current source.");
  }

  console.log("Firefox signed extension check passed.");
}

const command = process.argv[2] ?? "check";
if (command === "check") {
  await checkExtension();
} else if (command === "verify-signed") {
  await checkExtension();
  await verifySignedExtension();
} else {
  fail(`Unknown Firefox extension command: ${command}`);
}

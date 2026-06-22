import { readFile, stat } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const REPO_ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SOURCE_DIR = join(REPO_ROOT, "extensions", "firefox");
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

  console.log("Firefox extension check passed.");
}

const command = process.argv[2] ?? "check";
if (command !== "check") {
  fail(`Unknown Firefox extension command: ${command}`);
}

await checkExtension();

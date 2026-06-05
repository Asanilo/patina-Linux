import assert from "node:assert/strict";
import { readFileSync, unlinkSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { resolve } from "node:path";
import * as esbuild from "esbuild";
import { COPY } from "../src/shared/copy/uiText.ts";

const EXPECTED_VIEWS = [
  "dashboard",
  "history",
  "data",
  "mapping",
  "settings",
  "about",
] as const;

const EXPECTED_NAV_LABELS = [
  "今天",
  "历史",
  "数据",
  "应用",
  "设置",
  "关于",
] as const;

let passed = 0;
const require = createRequire(import.meta.url);

async function runTest(name: string, fn: () => Promise<void> | void) {
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

function readUtf8(path: string) {
  return readFileSync(path, "utf8");
}

function collectCopyKeyPaths(value: unknown, prefix = ""): string[] {
  if (typeof value === "function" || value === null || typeof value !== "object") {
    return [prefix];
  }

  if (Array.isArray(value)) {
    return [prefix];
  }

  return Object.entries(value).flatMap(([key, child]) => {
    const nextPrefix = prefix ? `${prefix}.${key}` : key;
    return collectCopyKeyPaths(child, nextPrefix);
  });
}

function tauriStubFor(path: string) {
  if (path === "@tauri-apps/api/window") {
    return `
      const noop = async () => {};
      const currentWindow = {
        minimize: noop,
        toggleMaximize: noop,
        close: noop,
        startDragging: noop,
        isMaximized: async () => false,
        isVisible: async () => true,
        isFocused: async () => true,
        onFocusChanged: async () => () => {},
        onResized: async () => () => {},
      };
      export function getCurrentWindow() {
        return currentWindow;
      }
    `;
  }

  if (path === "@tauri-apps/api/webviewWindow") {
    return `
      export function getCurrentWebviewWindow() {
        return { label: "main" };
      }
    `;
  }

  if (path === "@tauri-apps/api/core") {
    return `
      export async function invoke() {
        return null;
      }
      export class Channel {
        onmessage = null;
        constructor() {}
      }
    `;
  }

  if (path === "@tauri-apps/api/event") {
    return `
      export async function listen() {
        return () => {};
      }
      export async function emit() {}
    `;
  }

  if (path === "@tauri-apps/api/app") {
    return `
      export async function getVersion() {
        return "0.0.0-smoke";
      }
    `;
  }

  if (path === "@tauri-apps/plugin-opener") {
    return `
      export async function openUrl() {}
    `;
  }

  if (path === "@tauri-apps/plugin-sql") {
    return `
      export default class Database {
        static async load() {
          return new Database();
        }
        async select() {
          return [];
        }
        async execute() {}
        async close() {}
      }
    `;
  }

  throw new Error(`Missing Tauri smoke stub for ${path}`);
}

const tauriSmokeStubPlugin: esbuild.Plugin = {
  name: "tauri-smoke-stubs",
  setup(build) {
    build.onResolve({ filter: /^@tauri-apps\// }, (args) => ({
      path: args.path,
      namespace: "tauri-smoke-stub",
    }));
    build.onLoad({ filter: /.*/, namespace: "tauri-smoke-stub" }, (args) => ({
      contents: tauriStubFor(args.path),
      loader: "js",
    }));
  },
};

await runTest("app shell declares every primary desktop view", () => {
  const viewType = readUtf8("src/app/types/view.ts");
  const shell = readUtf8("src/app/AppShell.tsx");
  const sidebar = readUtf8("src/app/components/AppSidebar.tsx");

  for (const view of EXPECTED_VIEWS) {
    assert.match(viewType, new RegExp(`"${view}"`));
    assert.match(shell, new RegExp(`currentView === "${view}"`));
    assert.match(sidebar, new RegExp(`id: "${view}" as View`));
  }
});

await runTest("Chinese and English copy packages keep the same key structure", () => {
  assert.deepEqual(
    collectCopyKeyPaths(COPY["en-US"]).sort(),
    collectCopyKeyPaths(COPY["zh-CN"]).sort(),
  );
});

await runTest("app shell keeps History and Data snapshot loaders on their owning views", () => {
  const shell = readUtf8("src/app/AppShell.tsx");
  const historyBranch = shell.slice(shell.indexOf("<History"), shell.indexOf("<Data"));
  const dataBranch = shell.slice(shell.indexOf("<Data"), shell.indexOf("<Settings"));

  assert.match(historyBranch, /loadHistorySnapshot=\{loadHistoryRuntimeSnapshot\}/);
  assert.doesNotMatch(historyBranch, /loadDataTrendSnapshot=/);
  assert.match(dataBranch, /loadDataTrendSnapshot=\{loadDataTrendRuntimeSnapshot\}/);
  assert.doesNotMatch(dataBranch, /loadHistorySnapshot=/);
});

await runTest("Data regular view avoids visible loading and skeleton branches", () => {
  const data = readUtf8("src/features/data/components/Data.tsx");

  assert.doesNotMatch(data, /UI_TEXT\.history\.loading/);
  assert.doesNotMatch(data, /data-heatmap-skeleton/);
  assert.doesNotMatch(data, /aria-busy/);
});

await runTest("History regular view avoids visible loading copy", () => {
  const history = readUtf8("src/features/history/components/History.tsx");

  assert.doesNotMatch(history, /UI_TEXT\.history\.loading/);
  assert.doesNotMatch(history, /aria-busy/);
});

await runTest("operation-oriented pages keep explicit busy feedback", () => {
  const settings = readUtf8("src/features/settings/components/Settings.tsx");
  const mapping = readUtf8("src/features/classification/components/AppMapping.tsx");
  const dataSafety = readUtf8("src/features/settings/components/SettingsDataSafetyPanel.tsx");
  const updateDialog = readUtf8("src/features/update/components/UpdateConfirmDialog.tsx");

  assert.match(settings, /UI_TEXT\.settings\.loading/);
  assert.match(mapping, /UI_TEXT\.mapping\.loading/);
  assert.match(dataSafety, /backupExporting|backupRestoring/);
  assert.match(updateDialog, /UpdateProgressBar/);
  assert.match(updateDialog, /UI_TEXT\.update\.processing/);
});

await runTest("app shell uses feature-owned Data prewarm and heavy cache lifecycle exits", () => {
  const shell = readUtf8("src/app/AppShell.tsx");

  assert.match(shell, /prewarmDataFirstScreen/);
  assert.match(shell, /clearDataHeavyCaches/);
  assert.match(shell, /clearDataBootstrapCache/);
  assert.doesNotMatch(shell, /clearDataBootstrapSnapshot/);
  assert.doesNotMatch(shell, /buildDataTrendViewModel/);
  assert.doesNotMatch(shell, /buildActivityHeatmap/);
});

await runTest("app shell uses feature-owned page cache lifecycle exits", () => {
  const shell = readUtf8("src/app/AppShell.tsx");
  const cleanupEffect = shell.slice(shell.indexOf("if (isForegroundReady) return undefined;"), shell.indexOf("const handleMinSessionSecsChange"));

  assert.match(shell, /clearDashboardSnapshotCache/);
  assert.match(shell, /clearHistorySnapshotCache/);
  assert.match(shell, /includeDashboard: isDashboardRefreshEnabled/);
  assert.match(shell, /includeHistory: isHistoryRefreshEnabled/);
  assert.doesNotMatch(cleanupEffect, /clearDashboardSnapshotCache/);
  assert.match(cleanupEffect, /clearHistorySnapshotCache/);
  assert.match(cleanupEffect, /clearDataHeavyCaches/);
  assert.doesNotMatch(shell, /DASHBOARD_SNAPSHOT_CACHE/);
  assert.doesNotMatch(shell, /HISTORY_SNAPSHOT_CACHE/);
});

await runTest("app shell uses one fifteen minute threshold for long background behavior", () => {
  const policy = readUtf8("src/app/services/backgroundReturnHomePolicy.ts");
  const shell = readUtf8("src/app/AppShell.tsx");

  assert.match(policy, /LONG_BACKGROUND_DELAY_MS = 15 \* 60 \* 1000/);
  assert.doesNotMatch(shell, /10 \* 60 \* 1000/);
  assert.match(shell, /const BACKGROUND_CACHE_RELEASE_DELAY_MS = LONG_BACKGROUND_DELAY_MS/);
  assert.match(shell, /resetToDashboardAfterLongBackground/);
  assert.match(shell, /backgroundEnteredAtMsRef/);
});

await runTest("Dashboard first snapshot load is not gated by foreground refresh", () => {
  const hook = readUtf8("src/features/dashboard/hooks/useDashboardStats.ts");

  const firstLoadEffect = hook.slice(
    hook.indexOf("if (!classificationReady || hasRequestedInitialSnapshotRef.current) return;"),
    hook.indexOf("if (refreshKey === 0"),
  );
  const refreshEffect = hook.slice(
    hook.indexOf("if (refreshKey === 0"),
    hook.indexOf("const hasLiveSession"),
  );

  assert.doesNotMatch(firstLoadEffect, /foregroundRefreshEnabled/);
  assert.match(firstLoadEffect, /void loadSnapshot\(\)/);
  assert.match(refreshEffect, /foregroundRefreshEnabled/);
});

await runTest("update snapshot listener disposes if subscription resolves after unmount", () => {
  const hook = readUtf8("src/app/hooks/useUpdateState.ts");

  assert.match(hook, /if \(cancelled\) \{\s*dispose\(\);\s*return;\s*\}/);
});

await runTest("window foreground watcher composes and releases Tauri listeners", () => {
  const gateway = readUtf8("src/platform/desktop/windowControlGateway.ts");

  assert.match(gateway, /readCurrentWindowForegroundState/);
  assert.match(gateway, /onFocusChanged/);
  assert.match(gateway, /onResized/);
  assert.match(gateway, /unlisteners\.splice\(0\)/);
});

await runTest("app shell renders dashboard and primary navigation without Tauri runtime", async () => {
  const entry = `
    import React from "react";
    import { renderToString } from "react-dom/server";
    import AppShell from "./src/app/AppShell.tsx";

    const originalWarn = console.warn;
    console.warn = (...args) => {
      if (String(args[0] ?? "").includes("width(-1) and height(-1) of chart")) {
        return;
      }
      originalWarn(...args);
    };
    export const html = renderToString(React.createElement(AppShell));
    console.warn = originalWarn;
  `;
  const result = await esbuild.build({
    stdin: {
      contents: entry,
      resolveDir: process.cwd(),
      loader: "tsx",
    },
    bundle: true,
    write: false,
    platform: "node",
    format: "cjs",
    loader: {
      ".png": "dataurl",
      ".css": "empty",
    },
    plugins: [tauriSmokeStubPlugin],
  });
  const bundled = result.outputFiles[0]?.text;
  assert.ok(bundled);

  const bundlePath = "tests/.tmp-ui-smoke-bundle.cjs";
  writeFileSync(bundlePath, bundled, "utf8");
  let html = "";
  try {
    const module = require(resolve(bundlePath)) as { html: string };
    html = module.html;
  } finally {
    unlinkSync(bundlePath);
  }

  for (const label of EXPECTED_NAV_LABELS) {
    assert.ok(html.includes(`aria-label="${label}"`), `missing nav label ${label}`);
  }
  assert.ok(html.includes("专注分布"));
  assert.ok(html.includes("应用排行"));
  assert.ok(html.includes(`aria-label="按分类显示"`));
});

console.log(`Passed ${passed} UI smoke tests`);

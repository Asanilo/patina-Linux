import assert from "node:assert/strict";
import {
  buildSettingsDiagnosticsViewModel,
} from "../src/features/settings/services/settingsDiagnosticsViewModel.ts";
import type { TrackerHealthSnapshot } from "../src/shared/types/tracking.ts";

let passed = 0;

async function runTest(name: string, fn: () => void | Promise<void>) {
  try {
    await fn();
    passed += 1;
    console.log(`PASS ${name}`);
  } catch (error) {
    console.error(`FAIL ${name}`);
    console.error(error);
    process.exitCode = 1;
  }
}

const HEALTHY_GNOME: TrackerHealthSnapshot = {
  status: "healthy",
  lastHeartbeatMs: 1000,
  checkedAtMs: 1200,
  staleAfterMs: 5000,
  platformDiagnostics: {
    windowTracking: {
      status: "available",
      reason: null,
      provider: "gnome-shell-extension",
      sessionType: "wayland",
      desktop: "GNOME",
    },
  },
};

await runTest("settings diagnostics report available Linux window tracking and API metadata", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
  });

  assert.equal(items.find((item) => item.id === "window-tracking")?.value, "可用");
  assert.equal(items.find((item) => item.id === "window-tracking")?.tone, "ok");
  assert.equal(items.find((item) => item.id === "local-api")?.value, "http://127.0.0.1:14840");
});

await runTest("settings diagnostics warn when browser bridge is enabled but disconnected", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: true,
    webActivityPort: 18080,
    webActivityToken: "secret",
    webActivityBridge: {
      enabled: true,
      connected: false,
      browserClientId: null,
      browserKind: null,
      extensionVersion: null,
      lastActivityAtMs: null,
    },
  });

  const bridge = items.find((item) => item.id === "browser-bridge");
  assert.equal(bridge?.value, "未连接");
  assert.equal(bridge?.tone, "warning");
  assert.match(bridge?.detail ?? "", /18080/);
});

await runTest("settings diagnostics surface GNOME extension D-Bus failures", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: {
      ...HEALTHY_GNOME,
      platformDiagnostics: {
        windowTracking: {
          status: "unavailable",
          reason: "gnome-extension-dbus-unavailable",
          provider: "gnome-shell-extension",
          sessionType: "wayland",
          desktop: "GNOME",
        },
      },
    },
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
  });

  const windowTracking = items.find((item) => item.id === "window-tracking");
  assert.equal(windowTracking?.tone, "warning");
  assert.match(windowTracking?.detail ?? "", /GNOME 扩展 D-Bus 不可用/);
});

await runTest("settings diagnostics warn when local API is not listening", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    localApi: {
      baseUrl: "http://127.0.0.1:14840",
      tokenPath: "/home/user/.local/share/Patina/api_token",
      tokenPresent: true,
      listening: false,
    },
  });

  const localApi = items.find((item) => item.id === "local-api");
  assert.equal(localApi?.tone, "warning");
  assert.equal(localApi?.value, "未监听");
  assert.match(localApi?.detail ?? "", /api_token/);
});

console.log(`Passed ${passed} settings diagnostics view model tests`);

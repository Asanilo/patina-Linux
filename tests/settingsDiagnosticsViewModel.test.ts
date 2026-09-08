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

await runTest("settings diagnostics mark Linux autostart Exec failures as danger", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    desktopIntegration: {
      launchAtLogin: true,
      backgroundTrackingAtLogin: true,
      startMinimized: true,
      autostart: {
        path: "/home/user/.config/autostart/Patina.desktop",
        exists: true,
        exec: "/usr/local/bin/ghostty --autostart",
        valid: false,
        reason: "exec-not-patina",
      },
    },
  });

  const desktopIntegration = items.find((item) => item.id === "desktop-integration");
  assert.equal(desktopIntegration?.tone, "danger");
  assert.equal(desktopIntegration?.value, "自启动异常");
  assert.match(desktopIntegration?.detail ?? "", /ghostty --autostart/);
});

await runTest("settings diagnostics mark browser bridge disconnects as danger", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: true,
    webActivityPort: 18080,
    webActivityToken: "secret",
    webActivityBridge: {
      enabled: true,
      listening: true,
      connected: false,
      browserClientId: null,
      browserKind: null,
      extensionVersion: null,
      lastActivityAtMs: null,
    },
  });

  const bridge = items.find((item) => item.id === "browser-bridge");
  assert.equal(bridge?.value, "未连接");
  assert.equal(bridge?.tone, "danger");
  assert.match(bridge?.detail ?? "", /18080/);
});

await runTest("settings diagnostics distinguish browser listener failures", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: true,
    webActivityPort: 18080,
    webActivityToken: "secret",
    webActivityBridge: {
      enabled: true,
      listening: false,
      connected: false,
      browserClientId: null,
      browserKind: null,
      extensionVersion: null,
      lastActivityAtMs: null,
    },
  });

  const bridge = items.find((item) => item.id === "browser-bridge");
  assert.equal(bridge?.value, "监听异常");
  assert.equal(bridge?.tone, "danger");
  assert.match(bridge?.detail ?? "", /端口 18080 未成功监听/);
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
  assert.equal(windowTracking?.tone, "danger");
  assert.match(windowTracking?.detail ?? "", /GNOME 扩展 D-Bus 不可用/);
});

await runTest("settings diagnostics mark local API failures as danger without duplicated paths", () => {
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
  assert.equal(localApi?.tone, "danger");
  assert.equal(localApi?.value, "未监听");
  assert.equal(localApi?.detail, "不可连接 / Token 已生成");
  assert.equal(localApi?.metadata?.find((entry) => entry.label === "Token file")?.value, "/home/user/.local/share/Patina/api_token");
});

await runTest("settings diagnostics treat the installed disabled daemon unit as the expected preview state", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    daemonService: {
      serviceName: "patinad.service",
      managerAvailable: true,
      unitInstalled: true,
      unitFileState: "disabled",
      enabled: false,
      activeState: "inactive",
      subState: "dead",
      active: false,
      migrationState: "ready",
      migrationReason: "desktop autostart can be migrated later",
      controlAvailable: false,
      error: null,
      cutover: {
        state: "not-requested",
        requestId: null,
        updatedAtMs: null,
        failureCode: null,
        failureMessage: null,
        backgroundTrackingAtLogin: null,
      },
    },
  });

  const daemonService = items.find((item) => item.id === "daemon-service");
  assert.equal(daemonService?.value, "已安装 / 未启用");
  assert.equal(daemonService?.tone, "ok");
  assert.match(daemonService?.detail ?? "", /安全迁移条件/);
});

await runTest("settings diagnostics expose an early daemon activation as an owner conflict", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    daemonService: {
      serviceName: "patinad.service",
      managerAvailable: true,
      unitInstalled: true,
      unitFileState: "enabled",
      enabled: true,
      activeState: "failed",
      subState: "failed",
      active: false,
      migrationState: "owner-conflict",
      migrationReason: "desktop still owns tracking",
      controlAvailable: false,
      error: null,
      cutover: {
        state: "not-requested",
        requestId: null,
        updatedAtMs: null,
        failureCode: null,
        failureMessage: null,
        backgroundTrackingAtLogin: null,
      },
    },
  });

  const daemonService = items.find((item) => item.id === "daemon-service");
  assert.equal(daemonService?.value, "运行冲突");
  assert.equal(daemonService?.tone, "danger");
  assert.match(daemonService?.detail ?? "", /两个追踪进程/);
});

await runTest("settings diagnostics report a managed daemon as healthy", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    daemonService: {
      serviceName: "patinad.service",
      managerAvailable: true,
      unitInstalled: true,
      unitFileState: "enabled",
      enabled: true,
      activeState: "active",
      subState: "running",
      active: true,
      migrationState: "managed",
      migrationReason: "patinad.service is the active tracking owner for Patina Desktop",
      controlAvailable: false,
      error: null,
      cutover: {
        state: "completed",
        requestId: "cutover_test",
        updatedAtMs: 1000,
        failureCode: null,
        failureMessage: null,
        backgroundTrackingAtLogin: true,
      },
    },
  });

  const service = items.find((item) => item.id === "daemon-service");
  assert.equal(service?.value, "运行中");
  assert.equal(service?.tone, "ok");
  assert.match(service?.detail ?? "", /关闭桌面窗口不会停止记录/);
});

await runTest("settings diagnostics report an inactive managed daemon as blocked", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    daemonService: {
      serviceName: "patinad.service",
      managerAvailable: true,
      unitInstalled: true,
      unitFileState: "disabled",
      enabled: false,
      activeState: "inactive",
      subState: "dead",
      active: false,
      migrationState: "managed-blocked",
      migrationReason: "Patina Desktop is a daemon client but patinad.service is not active",
      controlAvailable: false,
      error: null,
      cutover: {
        state: "completed",
        requestId: "cutover_test",
        updatedAtMs: 1000,
        failureCode: null,
        failureMessage: null,
        backgroundTrackingAtLogin: false,
      },
    },
  });

  const service = items.find((item) => item.id === "daemon-service");
  assert.equal(service?.value, "已安装 / 未启用");
  assert.equal(service?.tone, "danger");
  assert.match(service?.detail ?? "", /追踪当前处于暂停状态/);
});

await runTest("settings diagnostics surface a failed runtime owner cutover", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    daemonService: {
      serviceName: "patinad.service",
      managerAvailable: true,
      unitInstalled: true,
      unitFileState: "enabled",
      enabled: true,
      activeState: "failed",
      subState: "failed",
      active: false,
      migrationState: "cutover-failed",
      migrationReason: "runtime owner cutover requires explicit repair",
      controlAvailable: false,
      error: null,
      cutover: {
        state: "failed",
        requestId: "cutover_test",
        updatedAtMs: 1000,
        failureCode: "daemon-not-ready",
        failureMessage: "timed out waiting for tracking readiness",
        backgroundTrackingAtLogin: true,
      },
    },
  });

  const service = items.find((item) => item.id === "daemon-service");
  assert.equal(service?.value, "接管失败");
  assert.equal(service?.tone, "danger");
  assert.match(service?.detail ?? "", /timed out waiting for tracking readiness/);
  assert.equal(service?.metadata?.find((entry) => entry.label === "Failure")?.value, "daemon-not-ready");
});

await runTest("settings diagnostics surface a daemon login preference mismatch", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    daemonService: {
      serviceName: "patinad.service",
      managerAvailable: true,
      unitInstalled: true,
      unitFileState: "disabled",
      enabled: false,
      activeState: "active",
      subState: "running",
      active: true,
      migrationState: "preference-mismatch",
      migrationReason: "saved preference does not match unit state",
      controlAvailable: true,
      error: null,
      cutover: {
        state: "completed",
        requestId: "cutover_test",
        updatedAtMs: 1000,
        failureCode: null,
        failureMessage: null,
        backgroundTrackingAtLogin: true,
      },
    },
  });

  const service = items.find((item) => item.id === "daemon-service");
  assert.equal(service?.value, "设置待对账");
  assert.equal(service?.tone, "warning");
  assert.match(service?.detail ?? "", /尚未与 patinad.service 状态一致/);
});

await runTest("settings diagnostics show an explicit embedded rollback", () => {
  const items = buildSettingsDiagnosticsViewModel({
    trackerHealth: HEALTHY_GNOME,
    webActivityEnabled: false,
    webActivityPort: 18080,
    webActivityToken: "",
    webActivityBridge: null,
    daemonService: {
      serviceName: "patinad.service",
      managerAvailable: true,
      unitInstalled: true,
      unitFileState: "disabled",
      enabled: false,
      activeState: "inactive",
      subState: "dead",
      active: false,
      migrationState: "embedded-rollback",
      migrationReason: "explicit embedded runtime fallback",
      controlAvailable: false,
      error: null,
      cutover: {
        state: "rolled-back",
        requestId: "cutover_test",
        updatedAtMs: 1000,
        failureCode: null,
        failureMessage: null,
        backgroundTrackingAtLogin: false,
      },
    },
  });

  const service = items.find((item) => item.id === "daemon-service");
  assert.equal(service?.value, "桌面内置追踪");
  assert.equal(service?.tone, "warning");
  assert.match(service?.detail ?? "", /关闭 Patina Desktop 会停止记录/);
});

console.log(`Passed ${passed} settings diagnostics view model tests`);

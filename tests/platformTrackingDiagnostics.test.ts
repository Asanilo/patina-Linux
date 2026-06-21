import assert from "node:assert/strict";
import {
  resolvePlatformTrackingDiagnosticMessage,
} from "../src/app/services/platformTrackingDiagnosticsService.ts";

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

await runTest("platform diagnostics stay quiet when window tracking is available", () => {
  assert.equal(resolvePlatformTrackingDiagnosticMessage({
    windowTracking: {
      status: "available",
      reason: null,
      provider: "gnome-shell-extension",
      sessionType: "wayland",
      desktop: "GNOME",
    },
  }), null);
});

await runTest("platform diagnostics explain missing GNOME extension D-Bus owner", () => {
  assert.equal(resolvePlatformTrackingDiagnosticMessage({
    windowTracking: {
      status: "unavailable",
      reason: "gnome-extension-dbus-unavailable",
      provider: "gnome-shell-extension",
      sessionType: "wayland",
      desktop: "GNOME",
    },
  }), "GNOME 扩展 D-Bus 不可用，当前无法可靠读取焦点窗口。");
});

await runTest("platform diagnostics explain unsupported Wayland compositors", () => {
  assert.equal(resolvePlatformTrackingDiagnosticMessage({
    windowTracking: {
      status: "unsupported",
      reason: "wayland-compositor-unsupported",
      provider: "none",
      sessionType: "wayland",
      desktop: "KDE",
    },
  }), "当前 Wayland 桌面暂未适配窗口追踪。");
});

console.log(`Passed ${passed} platform tracking diagnostics tests`);

import assert from "node:assert/strict";
import { buildSettingsDiagnosticsViewModel } from "../src/features/settings/services/settingsDiagnosticsViewModel.ts";
import {
  resolveDaemonServiceControlAvailability,
  canReloadDaemonVersion,
} from "../src/features/settings/services/settingsDaemonServiceControls.ts";
import type {
  DaemonServiceDiagnosticsSnapshot,
  RuntimeOwnerCutoverState,
} from "../src/platform/runtime/daemonServiceDiagnosticsGateway.ts";

let passed = 0;

function runTest(name: string, fn: () => void) {
  try {
    fn();
    passed += 1;
    console.log(`PASS ${name}`);
  } catch (error) {
    console.error(`FAIL ${name}`);
    console.error(error);
    process.exitCode = 1;
  }
}

function snapshot(
  state: RuntimeOwnerCutoverState,
  controlAvailable = true,
): DaemonServiceDiagnosticsSnapshot {
  return {
    serviceName: "patinad.service",
    managerAvailable: true,
    unitInstalled: true,
    unitFileState: "enabled",
    enabled: true,
    activeState: "active",
    subState: "running",
    active: true,
    migrationState: "managed",
    migrationReason: "test",
    controlAvailable,
    error: null,
    cutover: {
      state,
      requestId: "request-id",
      updatedAtMs: 1,
      failureCode: null,
      failureMessage: null,
      backgroundTrackingAtLogin: true,
    },
  };
}

runTest("completed cutover exposes login preference and rollback only", () => {
  assert.deepEqual(resolveDaemonServiceControlAvailability(snapshot("completed")), {
    backgroundLogin: true,
    retry: false,
    rollback: true,
  });
});

runTest("failed and blocked cutovers expose retry and rollback", () => {
  for (const state of ["failed", "blocked"] as const) {
    assert.deepEqual(resolveDaemonServiceControlAvailability(snapshot(state)), {
      backgroundLogin: false,
      retry: true,
      rollback: true,
    });
  }
});

runTest("interrupted rollback only exposes idempotent rollback", () => {
  assert.deepEqual(resolveDaemonServiceControlAvailability(snapshot("rolling-back")), {
    backgroundLogin: false,
    retry: false,
    rollback: true,
  });
});

runTest("completed rollback exposes an explicit re-enable action", () => {
  assert.deepEqual(resolveDaemonServiceControlAvailability(snapshot("rolled-back")), {
    backgroundLogin: false,
    retry: true,
    rollback: false,
  });
});

runTest("pending and unavailable states expose no mutation", () => {
  for (const state of ["prepared", "activating", "not-requested"] as const) {
    assert.deepEqual(resolveDaemonServiceControlAvailability(snapshot(state)), {
      backgroundLogin: false,
      retry: false,
      rollback: false,
    });
  }
  assert.deepEqual(resolveDaemonServiceControlAvailability(snapshot("completed", false)), {
    backgroundLogin: false,
    retry: false,
    rollback: false,
  });
  assert.deepEqual(resolveDaemonServiceControlAvailability(null), {
    backgroundLogin: false,
    retry: false,
    rollback: false,
  });
});

runTest("version reload requires a confirmed owner and a known version difference", () => {
  const value = snapshot("completed");
  value.version = { desktopVersion: "1.9.0-beta.8", runningVersion: "1.9.0-beta.7", restartAvailable: true, error: null };
  assert.equal(canReloadDaemonVersion(value), true);
  for (const update of [
    { runningVersion: value.version.desktopVersion }, { runningVersion: null },
    { restartAvailable: false }, { error: "offline" },
  ]) {
    assert.equal(canReloadDaemonVersion({ ...value, version: { ...value.version, ...update } }), false);
  }
  assert.equal(canReloadDaemonVersion({ ...value, active: false }), false);
  assert.equal(canReloadDaemonVersion({ ...value, controlAvailable: false }), false);
  assert.equal(canReloadDaemonVersion({ ...value, cutover: { ...value.cutover, state: "rolling-back" } }), false);
  assert.equal(canReloadDaemonVersion(null), false);
});

runTest("managed service version mismatch is a warning with both versions", () => {
  const value = snapshot("completed");
  value.version = { desktopVersion: "1.9.0-beta.8", runningVersion: "1.9.0-beta.7", restartAvailable: true, error: null };
  const input = {
    trackerHealth: { status: "healthy" as const, lastHeartbeatMs: 100, checkedAtMs: 100, staleAfterMs: 8000 },
    webActivityEnabled: false, webActivityPort: 12345, webActivityToken: "", webActivityBridge: null,
    daemonService: value,
  };
  const row = buildSettingsDiagnosticsViewModel(input).find(row => row.id === "daemon-service")!;
  assert.equal(row.value, "版本不一致");
  assert.equal(row.tone, "warning");
  assert.ok(row.metadata?.some(entry => entry.value === "1.9.0-beta.7"));
  assert.ok(row.metadata?.some(entry => entry.value === "1.9.0-beta.8"));
  value.version.runningVersion = value.version.desktopVersion;
  assert.equal(buildSettingsDiagnosticsViewModel(input).find(row => row.id === "daemon-service")?.tone, "ok");
  value.version.error = "offline";
  assert.equal(buildSettingsDiagnosticsViewModel(input).find(row => row.id === "daemon-service")?.tone, "warning");
});

if (!process.exitCode) {
  console.log(`${passed} settings daemon service control tests passed`);
}

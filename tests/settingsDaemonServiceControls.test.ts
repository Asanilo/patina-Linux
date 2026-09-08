import assert from "node:assert/strict";
import {
  resolveDaemonServiceControlAvailability,
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

runTest("pending, embedded, and unavailable states expose no mutation", () => {
  for (const state of ["prepared", "activating", "rolled-back", "not-requested"] as const) {
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

if (!process.exitCode) {
  console.log(`${passed} settings daemon service control tests passed`);
}

import assert from "node:assert/strict";
import { lstat, mkdtemp, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import {
  evaluateAcceptanceEvidence,
  parseSystemdProperties,
  summarizeCapabilities,
  summarizeCutoverReservation,
  writeEvidence,
} from "../scripts/patinad-installed-acceptance.ts";

function managedEvidence() {
  return {
    phase: "managed",
    expectedVersion: "1.9.0-beta.1",
    package: { installed: true, version: "1.9.0-beta.1" },
    installedFiles: {
      desktop: { exists: true, regular: true },
      daemon: { exists: true, regular: true },
      unit: { exists: true, regular: true },
    },
    systemd: { LoadState: "loaded", ActiveState: "active", ExecMainPID: "123" },
    cutover: { present: true, value: { state: "completed" } },
    runtimeLease: { present: true, value: { role: "daemon", pid: 123 } },
    host: { uid: 1000 },
    apiToken: { exists: true, regular: true, mode: "600", uid: 1000 },
    database: { exists: true, quickCheck: "ok" },
    api: {
      reachable: true,
      capabilities: {
        serverVersion: "1.9.0-beta.1",
        runtimeHost: "daemon",
        tracking: { owned: true, ready: true },
        daemonService: { owned: true, ready: true },
      },
    },
  };
}

function testSystemdPropertiesPreserveValuesContainingEquals() {
  assert.deepEqual(
    parseSystemdProperties("LoadState=loaded\nFragmentPath=/tmp/a=b.service\n"),
    { LoadState: "loaded", FragmentPath: "/tmp/a=b.service" },
  );
}

function testCapabilitySummaryKeepsOnlyAcceptanceFields() {
  const summary = summarizeCapabilities({
    data: {
      server_version: "1.9.0-beta.1",
      runtime_host: "daemon",
      protocol: {
        current: 2,
        min_supported_client: 1,
        max_supported_client: 2,
      },
      tracking: { owned: true, ready: true },
      browser_activity_bridge: { owned: true, ready: false },
      tools: { owned: true, ready: true },
      daemon_service: { owned: true, ready: true },
      write_api: { available: true, operations: ["runtime-settings"] },
      local_api_token: "must-not-survive",
      current_window: { title: "must-not-survive" },
    },
  });

  assert.equal(JSON.stringify(summary).includes("must-not-survive"), false);
  assert.equal(summary.runtimeHost, "daemon");
  assert.deepEqual(summary.tracking, { owned: true, ready: true });
  assert.equal(summary.writeApi.operationCount, 1);
}

function testManagedEvidenceRequiresOneReadyDaemonOwner() {
  const passing = evaluateAcceptanceEvidence(managedEvidence());
  assert.equal(passing.every((entry) => entry.status === "pass"), true);

  const failing = managedEvidence();
  failing.api.capabilities.tracking.ready = false;
  const failedChecks = evaluateAcceptanceEvidence(failing)
    .filter((entry) => entry.status === "fail")
    .map((entry) => entry.id);
  assert.deepEqual(failedChecks, ["tracking-ready"]);
}

function testCutoverSummaryMapsPersistedSnakeCaseFields() {
  assert.deepEqual(summarizeCutoverReservation({
    version: 1,
    status: "completed",
    profile: "production",
    request_id: "request-1",
    failure_code: null,
    background_tracking_at_login: true,
    desktop_launch_at_login: false,
    failure_message: "must-not-survive",
  }), {
    version: 1,
    state: "completed",
    profile: "production",
    requestId: "request-1",
    failureCode: null,
    backgroundTrackingAtLogin: true,
    desktopLaunchAtLogin: false,
  });
}

function testUninstallEvidenceRequiresDataButNoPackagePayload() {
  const checks = evaluateAcceptanceEvidence({
    host: { uid: 1000 },
    apiToken: { exists: true, regular: true, mode: "600", uid: 1000 },
    systemd: { ActiveState: "inactive", ExecMainPID: "0" },
    phase: "uninstalled",
    package: { installed: false },
    installedFiles: {
      desktop: { exists: false },
      daemon: { exists: false },
      unit: { exists: false },
    },
    database: { exists: true, quickCheck: "ok" },
  });

  assert.equal(checks.every((entry) => entry.status === "pass"), true);
}

function testBaselineDoesNotRequireDaemonPackageFiles() {
  const evidence = managedEvidence();
  evidence.phase = "baseline";
  evidence.expectedVersion = "1.8.3";
  evidence.package.version = "1.8.3";
  evidence.installedFiles.daemon = { exists: false, regular: false };
  evidence.installedFiles.unit = { exists: false, regular: false };

  const checks = evaluateAcceptanceEvidence(evidence);
  assert.equal(checks.every((entry) => entry.status === "pass"), true);
}

async function testEvidenceFilesAreOwnerOnlyAndNeverOverwritten() {
  const root = await mkdtemp(path.join(tmpdir(), "patina-installed-acceptance-"));
  const output = path.join(root, "evidence.json");
  try {
    await writeEvidence(output, "first\n");
    assert.equal((await lstat(output)).mode & 0o777, 0o600);
    await assert.rejects(() => writeEvidence(output, "second\n"), /EEXIST/);
    assert.equal(await readFile(output, "utf8"), "first\n");
  } finally {
    await rm(root, { recursive: true, force: true });
  }
}

function testRollbackEvidenceUsesPersistedStatus() {
  const evidence = managedEvidence();
  evidence.phase = "rolled-back";
  evidence.systemd.ActiveState = "inactive";
  evidence.systemd.ExecMainPID = "0";
  evidence.runtimeLease.value.role = "desktop";
  evidence.cutover.value = summarizeCutoverReservation({ status: "rolled_back" });
  assert.equal(evaluateAcceptanceEvidence(evidence).every(entry => entry.status === "pass"), true);
  evidence.cutover.value.state = "rolling_back";
  assert.equal(evaluateAcceptanceEvidence(evidence).find(entry => entry.id === "cutover-rolled-back")?.status, "fail");
}

function testManagedRejectsOldRuntimeAndMismatchedPid() {
  const evidence = managedEvidence();
  evidence.api.capabilities.serverVersion = "1.8.3";
  evidence.runtimeLease.value.pid = 456;
  const failed = evaluateAcceptanceEvidence(evidence).filter(e => e.status === "fail").map(e => e.id);
  assert.deepEqual(failed, ["daemon-service-pid", "daemon-version"]);
}

function testUnknownSystemdStateCannotPassAsStopped() {
  for (const phase of ["rolled-back", "uninstalled"]) {
    const evidence = { ...managedEvidence(), phase, systemd: { error: "bus unavailable" } };
    assert.equal(evaluateAcceptanceEvidence(evidence).find(e => e.id === "service-stopped")?.status, "fail");
  }
}

function testUninstallRequiresRetainedPrivateToken() {
  const evidence = { ...managedEvidence(), phase: "uninstalled" };
  evidence.apiToken.exists = false;
  assert.equal(evaluateAcceptanceEvidence(evidence).find(e => e.id === "api-token-retained")?.status, "fail");
  evidence.apiToken.exists = true;
  evidence.apiToken.mode = "644";
  assert.equal(evaluateAcceptanceEvidence(evidence).find(e => e.id === "api-token-retained")?.status, "fail");
}

testManagedRejectsOldRuntimeAndMismatchedPid();
testUnknownSystemdStateCannotPassAsStopped();
testUninstallRequiresRetainedPrivateToken();
testRollbackEvidenceUsesPersistedStatus();
testSystemdPropertiesPreserveValuesContainingEquals();
testCapabilitySummaryKeepsOnlyAcceptanceFields();
testManagedEvidenceRequiresOneReadyDaemonOwner();
testCutoverSummaryMapsPersistedSnakeCaseFields();
testUninstallEvidenceRequiresDataButNoPackagePayload();
testBaselineDoesNotRequireDaemonPackageFiles();
await testEvidenceFilesAreOwnerOnlyAndNeverOverwritten();

console.log("Passed 11 installed patinad acceptance tests");

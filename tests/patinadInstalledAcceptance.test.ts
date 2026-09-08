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
    systemd: { LoadState: "loaded", ActiveState: "active" },
    cutover: { present: true, value: { state: "completed" } },
    runtimeLease: { present: true, value: { role: "daemon" } },
    host: { uid: 1000 },
    apiToken: { exists: true, regular: true, mode: "600", uid: 1000 },
    database: { exists: true, quickCheck: "ok" },
    api: {
      reachable: true,
      capabilities: {
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

testSystemdPropertiesPreserveValuesContainingEquals();
testCapabilitySummaryKeepsOnlyAcceptanceFields();
testManagedEvidenceRequiresOneReadyDaemonOwner();
testCutoverSummaryMapsPersistedSnakeCaseFields();
testUninstallEvidenceRequiresDataButNoPackagePayload();
testBaselineDoesNotRequireDaemonPackageFiles();
await testEvidenceFilesAreOwnerOnlyAndNeverOverwritten();

console.log("Passed 7 installed patinad acceptance tests");

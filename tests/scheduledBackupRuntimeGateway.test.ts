import assert from "node:assert/strict";
import { parseScheduledBackupSnapshot } from "../src/platform/backup/scheduledBackupRuntimeGateway.ts";

const run = {
  runKey: "scheduled-backup:generation:2026-08-30:2100",
  targetGeneration: "generation",
  logicalDate: "2026-08-30",
  logicalTimeMinutes: 1260,
  targetPath: "/tmp/Patina-scheduled-backup-20260830-210000.zip",
  status: "succeeded",
  fileState: "present",
  attemptCount: 1,
  retryAtMs: null,
  startedAtMs: 1,
  completedAtMs: 2,
  archiveSha256: "abc",
  sizeBytes: 10,
  errorCode: null,
  errorMessage: null,
  cleanupWarning: null,
  updatedAtMs: 2,
};

const snapshot = parseScheduledBackupSnapshot({
  config: {
    enabled: true,
    cadence: "weekly",
    weekday: 5,
    localTimeMinutes: 1260,
    targetDir: "/tmp",
    targetGeneration: "generation",
    scheduleAnchorAtMs: 1,
    updatedAtMs: 1,
  },
  nextExecutionAtMs: 3,
  recentSuccess: run,
  recentFailure: null,
  activeRun: null,
});

assert.equal(snapshot.config.cadence, "weekly");
assert.equal(snapshot.recentSuccess?.targetPath, run.targetPath);

assert.throws(() => parseScheduledBackupSnapshot({
  ...snapshot,
  config: { ...snapshot.config, localTimeMinutes: "21:00" },
}));

console.log("PASS scheduled backup runtime gateway payload validation");

import assert from "node:assert/strict";
import { formatStorageBytes } from "../src/features/settings/services/storagePathDisplay.ts";
import {
  restoreDefaultStorageWithDeps,
  scheduleStorageMoveWithDeps,
} from "../src/features/settings/services/storageSettingsActions.ts";
import type {
  StorageMigrationPreview,
  StoragePendingMigration,
} from "../src/features/settings/services/storageSettingsActions.ts";

let passed = 0;

async function runTest(name: string, fn: () => Promise<void> | void) {
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

const preview: StorageMigrationPreview = {
  currentDataRoot: "/home/u/.local/share/Patina",
  targetDataRoot: "/mnt/work/Patina",
  currentWebviewRoot: "/home/u/.local/share/Patina",
  targetWebviewRoot: "/home/u/.local/share/Patina",
  payloadSizeBytes: 100,
  availableSpaceBytes: 1_000,
  requiredSpaceBytes: 200,
  requiresRestart: true,
};

const pending: StoragePendingMigration = {
  id: "migration-1",
  sourceDataRoot: preview.currentDataRoot,
  targetDataRoot: preview.targetDataRoot,
  sourceWebviewRoot: preview.currentWebviewRoot,
  targetWebviewRoot: preview.targetWebviewRoot,
  createdAtMs: 1,
};

await runTest("storage byte formatting uses stable binary units", () => {
  assert.equal(formatStorageBytes(0), "0 B");
  assert.equal(formatStorageBytes(1_024), "1.0 KiB");
  assert.equal(formatStorageBytes(1_572_864), "1.5 MiB");
});

await runTest("schedule flow previews before confirmation and mutation", async () => {
  const events: string[] = [];
  const result = await scheduleStorageMoveWithDeps("data", "/mnt/work", {
    preview: async () => {
      events.push("preview");
      return preview;
    },
    confirm: async () => {
      events.push("confirm");
      return true;
    },
    schedule: async () => {
      events.push("schedule");
      return pending;
    },
  });

  assert.deepEqual(events, ["preview", "confirm", "schedule"]);
  assert.equal(result.status, "scheduled");
  assert.equal(result.pending.id, "migration-1");
});

await runTest("cancelled confirmation never schedules a migration", async () => {
  let schedules = 0;
  const result = await scheduleStorageMoveWithDeps("webview", "/mnt/work", {
    preview: async () => preview,
    confirm: async () => false,
    schedule: async () => {
      schedules += 1;
      return pending;
    },
  });

  assert.equal(result.status, "cancelled");
  assert.equal(schedules, 0);
});

await runTest("restore default flow uses the explicit restore endpoints", async () => {
  const events: string[] = [];
  const result = await restoreDefaultStorageWithDeps("data", {
    preview: async () => {
      events.push("preview-default");
      return preview;
    },
    confirm: async () => {
      events.push("confirm");
      return true;
    },
    schedule: async () => {
      events.push("schedule-default");
      return pending;
    },
  });

  assert.deepEqual(events, ["preview-default", "confirm", "schedule-default"]);
  assert.equal(result.status, "scheduled");
});

await runTest("preview errors are propagated without confirmation", async () => {
  let confirmations = 0;
  await assert.rejects(
    scheduleStorageMoveWithDeps("data", "/missing", {
      preview: async () => {
        throw new Error("mount unavailable");
      },
      confirm: async () => {
        confirmations += 1;
        return true;
      },
      schedule: async () => pending,
    }),
    /mount unavailable/,
  );
  assert.equal(confirmations, 0);
});

console.log(`Passed ${passed} storage settings tests`);

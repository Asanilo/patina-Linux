import assert from "node:assert/strict";
import { startTrackerHealthPolling } from "../src/app/services/trackerHealthPollingService.ts";
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

function trackerHealth(checkedAtMs: number): TrackerHealthSnapshot {
  return {
    status: "healthy",
    lastHeartbeatMs: checkedAtMs,
    checkedAtMs,
    staleAfterMs: 8_000,
  };
}

function createScheduler() {
  const callbacks = new Map<number, () => void>();
  const cleared: number[] = [];
  let nextTimerId = 1;

  return {
    callbacks,
    cleared,
    clearInterval(timerId: number) {
      cleared.push(timerId);
      callbacks.delete(timerId);
    },
    setInterval(callback: () => void) {
      const timerId = nextTimerId;
      nextTimerId += 1;
      callbacks.set(timerId, callback);
      return timerId;
    },
  };
}

await runTest("tracker health polling refreshes immediately and on interval", async () => {
  const scheduler = createScheduler();
  const snapshots: TrackerHealthSnapshot[] = [];
  const loadCalls: number[] = [];
  let nowMs = 1_000;

  const stop = startTrackerHealthPolling((snapshot) => {
    snapshots.push(snapshot);
  }, {
    deps: {
      clearInterval: scheduler.clearInterval,
      loadSnapshot: async (requestedNowMs) => {
        loadCalls.push(requestedNowMs);
        return trackerHealth(requestedNowMs);
      },
      now: () => nowMs,
      setInterval: scheduler.setInterval,
      warn: () => {},
    },
    intervalMs: 25,
  });

  await Promise.resolve();
  assert.deepEqual(loadCalls, [1_000]);
  assert.equal(snapshots[0].checkedAtMs, 1_000);

  nowMs = 2_000;
  scheduler.callbacks.get(1)?.();
  await Promise.resolve();
  assert.deepEqual(loadCalls, [1_000, 2_000]);
  assert.equal(snapshots[1].checkedAtMs, 2_000);

  stop();
  assert.deepEqual(scheduler.cleared, [1]);
});

await runTest("tracker health polling ignores pending refresh after stop", async () => {
  const scheduler = createScheduler();
  const snapshots: TrackerHealthSnapshot[] = [];
  let resolveLoad: ((snapshot: TrackerHealthSnapshot) => void) | null = null;

  const stop = startTrackerHealthPolling((snapshot) => {
    snapshots.push(snapshot);
  }, {
    deps: {
      clearInterval: scheduler.clearInterval,
      loadSnapshot: () => new Promise<TrackerHealthSnapshot>((resolve) => {
        resolveLoad = resolve;
      }),
      now: () => 3_000,
      setInterval: scheduler.setInterval,
      warn: () => {},
    },
  });

  stop();
  resolveLoad?.(trackerHealth(3_000));
  await Promise.resolve();
  await Promise.resolve();

  assert.deepEqual(snapshots, []);
  assert.deepEqual(scheduler.cleared, [1]);
});

await runTest("tracker health polling reports load failures without stopping interval cleanup", async () => {
  const scheduler = createScheduler();
  const warnings: Array<{ message: string; error: unknown }> = [];

  const stop = startTrackerHealthPolling(() => {
    throw new Error("snapshot should not be delivered");
  }, {
    deps: {
      clearInterval: scheduler.clearInterval,
      loadSnapshot: async () => {
        throw new Error("load failed");
      },
      now: () => 4_000,
      setInterval: scheduler.setInterval,
      warn: (message, error) => {
        warnings.push({ message, error });
      },
    },
  });

  await Promise.resolve();
  assert.equal(warnings.length, 1);
  assert.equal(warnings[0].message, "load tracker health failed");

  stop();
  assert.deepEqual(scheduler.cleared, [1]);
});

console.log(`Passed ${passed} tracker health polling service tests`);

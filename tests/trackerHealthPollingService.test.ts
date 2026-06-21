import assert from "node:assert/strict";
import { loadTrackerHealthSnapshotWithDeps } from "../src/app/services/appRuntimeBootstrapService.ts";
import { startTrackerHealthPolling } from "../src/app/services/trackerHealthPollingService.ts";
import { parseTrackerHealthRuntimeSnapshot } from "../src/platform/runtime/trackingRawDtos.ts";
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

await runTest("tracker health polling skips interval ticks while a refresh is pending", async () => {
  const scheduler = createScheduler();
  const warnings: Array<{ message: string; error: unknown }> = [];
  const loadCalls: number[] = [];
  const snapshots: TrackerHealthSnapshot[] = [];
  let nowMs = 5_000;
  let resolveLoad: ((snapshot: TrackerHealthSnapshot) => void) | null = null;

  const stop = startTrackerHealthPolling((snapshot) => {
    snapshots.push(snapshot);
  }, {
    deps: {
      clearInterval: scheduler.clearInterval,
      loadSnapshot: (requestedNowMs) => {
        loadCalls.push(requestedNowMs);
        return new Promise<TrackerHealthSnapshot>((resolve) => {
          resolveLoad = resolve;
        });
      },
      now: () => nowMs,
      setInterval: scheduler.setInterval,
      warn: (message, error) => {
        warnings.push({ message, error });
      },
    },
  });

  assert.deepEqual(loadCalls, [5_000]);

  nowMs = 6_000;
  scheduler.callbacks.get(1)?.();
  await Promise.resolve();
  assert.deepEqual(loadCalls, [5_000]);
  assert.deepEqual(warnings, []);

  resolveLoad?.(trackerHealth(5_000));
  await Promise.resolve();
  assert.deepEqual(snapshots.map((snapshot) => snapshot.checkedAtMs), [5_000]);

  nowMs = 7_000;
  scheduler.callbacks.get(1)?.();
  assert.deepEqual(loadCalls, [5_000, 7_000]);

  stop();
});

await runTest("tracker health polling resumes interval refreshes after a pending failure", async () => {
  const scheduler = createScheduler();
  const warnings: Array<{ message: string; error: unknown }> = [];
  const loadCalls: number[] = [];
  let nowMs = 8_000;
  let rejectLoad: ((error: Error) => void) | null = null;

  const stop = startTrackerHealthPolling(() => {
    throw new Error("snapshot should not be delivered");
  }, {
    deps: {
      clearInterval: scheduler.clearInterval,
      loadSnapshot: (requestedNowMs) => {
        loadCalls.push(requestedNowMs);
        if (loadCalls.length === 1) {
          return new Promise<TrackerHealthSnapshot>((_resolve, reject) => {
            rejectLoad = reject;
          });
        }

        return Promise.reject(new Error("second load failed"));
      },
      now: () => nowMs,
      setInterval: scheduler.setInterval,
      warn: (message, error) => {
        warnings.push({ message, error });
      },
    },
  });

  nowMs = 9_000;
  scheduler.callbacks.get(1)?.();
  await Promise.resolve();
  assert.deepEqual(loadCalls, [8_000]);
  assert.deepEqual(warnings, []);

  rejectLoad?.(new Error("first load failed"));
  await Promise.resolve();
  assert.equal(warnings.length, 1);
  assert.equal(warnings[0].message, "load tracker health failed");

  nowMs = 10_000;
  scheduler.callbacks.get(1)?.();
  await Promise.resolve();
  assert.deepEqual(loadCalls, [8_000, 10_000]);
  assert.equal(warnings.length, 2);

  stop();
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

await runTest("tracker health runtime raw snapshot parser maps valid payloads", () => {
  assert.deepEqual(parseTrackerHealthRuntimeSnapshot({
    last_heartbeat_ms: 11_000,
    last_successful_sample_ms: 10_500,
    last_watchdog_seal_sample_ms: null,
    platform_diagnostics: {
      window_tracking: {
        status: "unavailable",
        reason: "gnome-extension-dbus-unavailable",
        provider: "gnome-shell-extension",
        session_type: "wayland",
        desktop: "GNOME",
      },
    },
  }), {
    lastHeartbeatMs: 11_000,
    lastSuccessfulSampleMs: 10_500,
    lastWatchdogSealSampleMs: null,
    platformDiagnostics: {
      windowTracking: {
        status: "unavailable",
        reason: "gnome-extension-dbus-unavailable",
        provider: "gnome-shell-extension",
        sessionType: "wayland",
        desktop: "GNOME",
      },
    },
  });

  assert.equal(parseTrackerHealthRuntimeSnapshot({
    last_heartbeat_ms: "11_000",
    last_successful_sample_ms: 10_500,
    last_watchdog_seal_sample_ms: null,
  }), null);
});

await runTest("tracker health snapshot prefers Rust runtime health over stored heartbeat", async () => {
  const warnings: string[] = [];

  const snapshot = await loadTrackerHealthSnapshotWithDeps(12_000, {
    getTrackerHealthRuntimeSnapshot: async () => ({
      lastHeartbeatMs: 11_000,
      lastSuccessfulSampleMs: 11_000,
      lastWatchdogSealSampleMs: null,
      platformDiagnostics: {
        windowTracking: {
          status: "unavailable",
          reason: "gnome-extension-dbus-unavailable",
          provider: "gnome-shell-extension",
          sessionType: "wayland",
          desktop: "GNOME",
        },
      },
    }),
    loadTrackerHealthTimestampMs: async () => {
      throw new Error("stored heartbeat should not be read");
    },
    warn: (message) => {
      warnings.push(message);
    },
  });

  assert.equal(snapshot.status, "healthy");
  assert.equal(snapshot.lastHeartbeatMs, 11_000);
  assert.equal(snapshot.platformDiagnostics?.windowTracking.reason, "gnome-extension-dbus-unavailable");
  assert.deepEqual(warnings, []);
});

await runTest("tracker health snapshot falls back to stored heartbeat when runtime health is unavailable", async () => {
  const warnings: string[] = [];

  const snapshot = await loadTrackerHealthSnapshotWithDeps(20_000, {
    getTrackerHealthRuntimeSnapshot: async () => null,
    loadTrackerHealthTimestampMs: async () => 19_000,
    warn: (message) => {
      warnings.push(message);
    },
  });

  assert.equal(snapshot.status, "healthy");
  assert.equal(snapshot.lastHeartbeatMs, 19_000);
  assert.deepEqual(warnings, [
    "Falling back to stored tracker heartbeat; runtime health snapshot unavailable",
  ]);
});

console.log(`Passed ${passed} tracker health polling service tests`);

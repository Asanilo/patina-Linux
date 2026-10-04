import assert from "node:assert/strict";
import { loadHistorySnapshot } from "../src/features/history/services/historyReadModel.ts";
import type { HistorySession } from "../src/shared/types/sessions.ts";

function makeSession(overrides: Partial<HistorySession> = {}): HistorySession {
  const startTime = overrides.startTime ?? new Date(2026, 0, 2, 9, 0, 0, 0).getTime();
  const endTime = Object.hasOwn(overrides, "endTime")
    ? overrides.endTime!
    : startTime + 60 * 60_000;

  return {
    id: overrides.id ?? 1,
    appName: overrides.appName ?? "VSCodium",
    exeName: overrides.exeName ?? "vscodium.exe",
    windowTitle: overrides.windowTitle ?? "Work",
    startTime,
    endTime,
    duration: overrides.duration ?? (endTime === null ? null : endTime - startTime),
    continuityGroupStartTime: overrides.continuityGroupStartTime ?? startTime,
    titleSampleDetails: overrides.titleSampleDetails ?? [],
  };
}

let passed = 0;

async function runTest(name: string, fn: () => Promise<void>) {
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

await runTest("history snapshot keeps app sessions when optional web reads fail", async () => {
  const daySession = makeSession({ id: 1 });
  let reads = 0;
  const originalWarn = console.warn;
  let warning = "";
  console.warn = (message?: unknown) => {
    warning = String(message ?? "");
  };

  try {
    const snapshot = await loadHistorySnapshot(new Date(2026, 0, 2), {
      getHistoryProduct: async (fromMs, toMs) => {
        reads++;
        assert.equal(fromMs, new Date(2026, 0, 2).getTime());
        assert.equal(toMs, new Date(2026, 0, 3).getTime());
        return {fromMs, toMs, sampledAtMs: toMs, configurationRevision: "a".repeat(64),
          trackingHealth: {status: "healthy", lastHeartbeatMs: toMs, liveCutoffMs: toMs, staleAfterMs: 8000},
          hours: Array.from({length:24},(_,hour)=>({hour,duration:hour===9?3600000:0,categories:hour===9?[{category:"development",duration:3600000}]:[]})),
          sessions: [daySession]};
      },
      getWebActivitySegmentsInRange: async () => {
        throw new Error("no such table: web_activity_segments");
      },
      loadWebDomainOverrides: async () => ({
        "github.com": {
          displayName: "GitHub",
        },
      }),
    });

    assert.deepEqual(snapshot.daySessions, [daySession]);
    assert.equal(reads, 1);
    assert.deepEqual(snapshot.dayWebSegments, []);
    assert.deepEqual(snapshot.webDomainOverrides, {});
    assert.match(warning, /History web activity data is unavailable/);
  } finally {
    console.warn = originalWarn;
  }
});

console.log(`Passed ${passed} history read model tests`);

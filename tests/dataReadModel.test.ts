import assert from "node:assert/strict";
import { dailyAppsFixture } from "./helpers/dailyAppsFixture.ts";
import { readFile } from "node:fs/promises";
import { getDailyActivity } from "../src/platform/persistence/dailyActivityRepository.ts";
import { ProcessMapper } from "../src/shared/classification/processMapper.ts";
import { mapRawAggregateSessionCandidates } from "../src/platform/persistence/sessionReadRepository.ts";
import {
  buildActivityHeatmap,
  aggregateHeatmapDays,
  buildDailyActivityHeatmap,
  buildDataTrendViewModel,
  buildDataAppTrendViewModel,
  buildYearOptions,
  getDataHeatmapDayCacheSizeForTests,
  getCachedDataHeatmapDays,
  getCachedEarliestSessionStartTime,
  clearDataReadModelCache,
  getHeatmapRange,
  loadDataHeatmapSnapshot as loadDailyHeatmapSnapshot,
  prewarmRecentDataHeatmapCache as prewarmDailyHeatmapCache,
  resetDataReadModelCacheForTests,
  type AggregateSessionRecord,
  type HeatmapSelection,
} from "../src/features/data/services/dataReadModel.ts";
import { clearDataHeavyCaches } from "../src/features/data/services/dataCacheLifecycle.ts";
import {
  prewarmDataFirstScreen,
  resetDataFirstScreenPrewarmForTests,
} from "../src/features/data/services/dataFirstScreenPrewarm.ts";
import {
  clearDataTrendSnapshotCache,
  getDataTrendSnapshotCacheSizeForTests,
  loadDataTrendSnapshot,
  type DataTrendSnapshot,
} from "../src/features/data/services/dataTrendSnapshot.ts";
import {
  loadPersistedDataBootstrapSnapshot,
  resetDataBootstrapSnapshotForTests,
  saveDataBootstrapSnapshot,
  type DataBootstrapSnapshot,
} from "../src/features/data/services/dataBootstrapSnapshot.ts";

let passed = 0;

// Legacy fact fixtures are aggregated inside the test, never by the production loader.
interface DataHeatmapDependencies {
  getEarliestSessionStartTime: () => Promise<number | null>;
  getSessionsInRange: (start: number, end: number) => Promise<AggregateSessionRecord[]>;
}
function dailyFixtureDeps(selection: HeatmapSelection, now: number, deps: DataHeatmapDependencies) {
  return { getDailyActivity: async (start: number, end: number) => {
    const [earliestStartTime, sessions] = await Promise.all([
      deps.getEarliestSessionStartTime(), deps.getSessionsInRange(start, end),
    ]);
    return { earliestStartTime, days: aggregateHeatmapDays(sessions, selection, now) };
  } };
}
function loadDataHeatmapSnapshot(selection: HeatmapSelection, now: number, deps: DataHeatmapDependencies) {
  return loadDailyHeatmapSnapshot(selection, now, dailyFixtureDeps(selection, now, deps));
}
function prewarmRecentDataHeatmapCache(now: number, deps: DataHeatmapDependencies) {
  return prewarmDailyHeatmapCache(now, dailyFixtureDeps("recent", now, deps));
}

async function runTest(name: string, fn: () => Promise<void> | void) {
  resetDataBootstrapSnapshotForTests();
  resetDataFirstScreenPrewarmForTests();
  clearDataTrendSnapshotCache();
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

await runTest("daily gateway validates complete local days without SQL fallback", async () => {
  const start = new Date(2026, 0, 1).getTime();
  const middle = new Date(2026, 0, 2).getTime();
  const end = new Date(2026, 0, 3).getTime();
  const response = { sampled_at_ms: end, earliest_start_ms: start, days: [
    { start_ms: start, end_ms: middle, active_ms: 123 },
    { start_ms: middle, end_ms: end, active_ms: 456 },
  ] };
  let calls = 0;
  const result = await getDailyActivity(start, end, async (from, to) => {
    calls += 1;
    assert.equal(from, "2026-01-01");
    assert.equal(to, "2026-01-03");
    return response;
  });
  assert.deepEqual(result.days, [{ date: "2026-01-01", duration: 123 }, { date: "2026-01-02", duration: 456 }]);
  for (const invalid of [null, {}, { ...response, days: [] },
    { ...response, days: response.days.slice().reverse() },
    { ...response, days: [{ ...response.days[0], start_ms: start + 1 }, response.days[1]] },
    { ...response, days: [{ ...response.days[0], active_ms: -1 }, response.days[1]] },
    { ...response, sampled_at_ms: Number.NaN },
  ]) {
    await assert.rejects(getDailyActivity(start, end, async () => invalid));
  }
  await assert.rejects(getDailyActivity(start, end, async () => { calls += 1; throw new Error("heatmap-unsupported"); }), /heatmap-unsupported/);
  assert.equal(calls, 2);
  await assert.rejects(getDailyActivity(start + 1, end, async () => { throw new Error("unexpected request"); }), /midnights/);
  const source = await readFile(new URL("../src/features/data/services/dataReadModel.ts", import.meta.url), "utf8");
  assert.ok(!source.includes("getSessionSummariesInRange"));
  assert.ok(!source.includes("getSessionsInRange"));
});

await runTest("daily snapshot consumes backend totals and refreshes earliest activity atomically", async () => {
  resetDataReadModelCacheForTests();
  let earliest = 1;
  const now = new Date(2026, 0, 3).getTime();
  const deps = { getDailyActivity: async () => ({ earliestStartTime: earliest, days: [{ date: "2026-01-01", duration: 37 }] }) };
  const first = await loadDailyHeatmapSnapshot(2026, now, deps);
  assert.equal(first.days[0].duration, 37);
  earliest = 2;
  assert.equal((await loadDailyHeatmapSnapshot(2026, now, deps)).earliestStartTime, 2);
});

await runTest("bootstrap rejects snapshots produced by the legacy heatmap compiler", async () => {
  let cleared = false;
  const result = await loadPersistedDataBootstrapSnapshot({
    loadPayload: async () => JSON.stringify(makeBootstrapSnapshot()),
    clearPayload: async () => { cleared = true; },
  });
  assert.equal(result, null);
  assert.equal(cleared, true);
});

await runTest("bootstrap rejects legacy overview totals even with current heatmap version", async () => {
  let cleared = false;
  const result = await loadPersistedDataBootstrapSnapshot({
    loadPayload: async () => JSON.stringify({ ...makeBootstrapSnapshot(), heatmapReadVersion: 2 }),
    clearPayload: async () => { cleared = true; },
  });
  assert.equal(result, null);
  assert.equal(cleared, true);
});

function makeSession(overrides: Partial<AggregateSessionRecord>): AggregateSessionRecord {
  return {
    appName: "Cursor",
    exeName: "cursor.exe",
    startTime: 0,
    endTime: 0,
    ...overrides,
  };
}

function findCell(rows: ReturnType<typeof buildActivityHeatmap>, date: string) {
  return rows.flatMap((week) => week.cells).find((cell) => cell.date === date);
}

function makeBootstrapSnapshot(overrides: Partial<DataBootstrapSnapshot> = {}): DataBootstrapSnapshot {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const sessions = [
    makeSession({
      startTime: new Date(2026, 4, 8, 9, 0, 0).getTime(),
      endTime: new Date(2026, 4, 8, 10, 0, 0).getTime(),
    }),
  ];
  const overviewRange = 7;
  const appRange = 7;
  const overviewTrendViewModel = buildDataTrendViewModel(sessions, overviewRange, nowMs);
  const appTrendViewModel = buildDataAppTrendViewModel(sessions, appRange, nowMs, null);

  return {
    createdAtMs: nowMs,
    overviewRangeCacheKey: "rolling:7:2026-05-02:2026-05-08",
    appRangeCacheKey: "rolling:7:2026-05-02:2026-05-08",
    heatmapSelection: "recent",
    mappingVersion: 0,
    uiLanguage: "zh-CN",
    overviewTrendViewModel,
    appTrendViewModel,
    heatmapRows: buildActivityHeatmap(sessions, "recent", nowMs),
    earliestStartTime: sessions[0].startTime,
    ...overrides,
  };
}

await runTest("activity heatmap splits sessions across local days", () => {
  const nowMs = new Date(2026, 0, 3, 12, 0, 0).getTime();
  const rows = buildActivityHeatmap([
    makeSession({
      startTime: new Date(2026, 0, 1, 23, 0, 0).getTime(),
      endTime: new Date(2026, 0, 2, 1, 30, 0).getTime(),
    }),
  ], 2026, nowMs);

  assert.equal(findCell(rows, "2026-01-01")?.duration, 60 * 60 * 1000);
  assert.equal(findCell(rows, "2026-01-02")?.duration, 90 * 60 * 1000);
});

await runTest("activity heatmap suppresses intensity for future and outside-year cells", () => {
  const nowMs = new Date(2026, 0, 3, 12, 0, 0).getTime();
  const rows = buildActivityHeatmap([
    makeSession({
      startTime: new Date(2026, 0, 4, 10, 0, 0).getTime(),
      endTime: new Date(2026, 0, 4, 11, 0, 0).getTime(),
    }),
  ], 2026, nowMs);
  const future = findCell(rows, "2026-01-04");
  const outsideYear = findCell(rows, "2025-12-29");

  assert.equal(future?.isFuture, true);
  assert.equal(future?.intensity, 0);
  assert.equal(outsideYear?.isOutsideYear, true);
  assert.equal(outsideYear?.intensity, 0);
});

await runTest("activity heatmap keeps empty ranges renderable", () => {
  const nowMs = new Date(2026, 0, 3, 12, 0, 0).getTime();
  const rows = buildActivityHeatmap([], 2026, nowMs);
  const visibleDay = findCell(rows, "2026-01-02");

  assert.ok(rows.length > 0);
  assert.equal(visibleDay?.duration, 0);
  assert.equal(visibleDay?.intensity, 0);
});

await runTest("activity heatmap labels sub-second durations as zero minutes", () => {
  const nowMs = new Date(2026, 0, 3, 12, 0, 0).getTime();
  const rows = buildActivityHeatmap([
    makeSession({
      startTime: new Date(2026, 0, 2, 9, 0, 0, 0).getTime(),
      endTime: new Date(2026, 0, 2, 9, 0, 0, 999).getTime(),
    }),
  ], 2026, nowMs);

  assert.equal(findCell(rows, "2026-01-01")?.label, "01/01 · 0m");
  assert.equal(findCell(rows, "2026-01-02")?.label, "01/02 · 0m");
});

await runTest("year options include every year from current back to earliest activity", () => {
  assert.deepEqual(
    buildYearOptions(new Date(2024, 6, 1).getTime(), 2026),
    [2026, 2025, 2024],
  );
  assert.deepEqual(buildYearOptions(null, 2026), [2026]);
});

await runTest("activity trend exposes dates only for day granularity", () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const sessions = [
    makeSession({
      startTime: new Date(2026, 4, 7, 9, 0, 0).getTime(),
      endTime: new Date(2026, 4, 7, 10, 0, 0).getTime(),
    }),
  ];
  const weekly = buildDataTrendViewModel(sessions, 7, nowMs);
  const monthly = buildDataTrendViewModel(sessions, 30, nowMs);
  const yearly = buildDataTrendViewModel(sessions, 365, nowMs);

  assert.equal(weekly.granularity, "day");
  assert.equal(monthly.granularity, "day");
  assert.equal(yearly.granularity, "month");
  assert.equal(weekly.chartData.at(-2)?.date, "2026-05-07");
  assert.match(monthly.chartData.at(-1)?.date ?? "", /^\d{4}-\d{2}-\d{2}$/);
  assert.equal(yearly.chartData.at(-1)?.date, null);
});

await runTest("app trend groups sessions by application and day", () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const rows = buildDataAppTrendViewModel([
    makeSession({
      appName: "Blender",
      exeName: "blender.exe",
      startTime: new Date(2026, 4, 6, 10, 0, 0).getTime(),
      endTime: new Date(2026, 4, 6, 12, 0, 0).getTime(),
    }),
    makeSession({
      appName: "Blender",
      exeName: "blender.exe",
      startTime: new Date(2026, 4, 7, 9, 0, 0).getTime(),
      endTime: new Date(2026, 4, 7, 10, 30, 0).getTime(),
    }),
    makeSession({
      appName: "Cursor",
      exeName: "cursor.exe",
      startTime: new Date(2026, 4, 7, 14, 0, 0).getTime(),
      endTime: new Date(2026, 4, 7, 15, 0, 0).getTime(),
    }),
  ], 7, nowMs, null);
  const may7 = rows.dayRows.find((row) => row.date === "2026-05-07");

  assert.equal(rows.selectedApp?.appName, "Blender");
  assert.equal(rows.granularity, "day");
  assert.equal(rows.selectedApp?.totalDuration, 210 * 60 * 1000);
  assert.equal(rows.selectedApp?.activeDayCount, 2);
  assert.equal(rows.dayRows.length, 7);
  assert.equal(may7?.duration, 90 * 60 * 1000);
  assert.equal(rows.peakDay?.date, "2026-05-06");
});

await runTest("app trend preserves explicit selected application", () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const rows = buildDataAppTrendViewModel([
    makeSession({
      appName: "Blender",
      exeName: "blender.exe",
      startTime: new Date(2026, 4, 8, 10, 0, 0).getTime(),
      endTime: new Date(2026, 4, 8, 11, 0, 0).getTime(),
    }),
    makeSession({
      appName: "Cursor",
      exeName: "cursor.exe",
      startTime: new Date(2026, 4, 8, 8, 0, 0).getTime(),
      endTime: new Date(2026, 4, 8, 11, 0, 0).getTime(),
    }),
  ], 7, nowMs, "blender.exe");

  assert.equal(rows.selectedApp?.appName, "Blender");
  assert.equal(rows.selectedApp?.totalDuration, 60 * 60 * 1000);
  assert.equal(rows.chartData.at(-1)?.duration, 60 * 60 * 1000);
});

await runTest("app trend merges duplicate display options", () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const rows = buildDataAppTrendViewModel([
    makeSession({
      appName: "Antigravity",
      exeName: "antigravity.exe",
      startTime: new Date(2026, 4, 8, 10, 0, 0).getTime(),
      endTime: new Date(2026, 4, 8, 10, 0, 22).getTime(),
    }),
    makeSession({
      appName: "Antigravity",
      exeName: "Antigravity.exe",
      startTime: new Date(2026, 4, 8, 11, 0, 0).getTime(),
      endTime: new Date(2026, 4, 8, 11, 0, 22).getTime(),
    }),
  ], 7, nowMs, null);

  assert.equal(rows.appOptions.length, 1);
  assert.equal(rows.selectedApp?.appName, "Antigravity");
  assert.equal(rows.selectedApp?.totalDuration, 44 * 1000);
  assert.equal(rows.chartData.at(-1)?.duration, 44 * 1000);
});

await runTest("yearly app trend averages by month", () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const rows = buildDataAppTrendViewModel([
    makeSession({
      appName: "Blender",
      exeName: "blender.exe",
      startTime: new Date(2026, 3, 8, 10, 0, 0).getTime(),
      endTime: new Date(2026, 3, 8, 22, 0, 0).getTime(),
    }),
  ], 365, nowMs, "blender.exe");

  assert.equal(rows.granularity, "month");
  assert.equal(rows.selectedApp?.averageDuration, 60 * 60 * 1000);
});

await runTest("aggregate repository mapping keeps a minimal effective time slice", () => {
  const rows = mapRawAggregateSessionCandidates([{
    app_name: "Cursor",
    exe_name: "cursor.exe",
    window_title: "README.md",
    start_time: 10_000,
    effective_end_time: 8_000,
  }]);

  assert.deepEqual(rows, [{
    appName: "Cursor",
    exeName: "cursor.exe",
    startTime: 10_000,
    endTime: 10_000,
  }]);
  assert.deepEqual(Object.keys(rows[0]).sort(), ["appName", "endTime", "exeName", "startTime"]);
});

await runTest("aggregate repository mapping filters legacy lifecycle noise using title metadata", () => {
  const rows = mapRawAggregateSessionCandidates([
    {
      app_name: "Alma",
      exe_name: "alma-0.0.750-win-x64.exe",
      window_title: "Alma 安装",
      start_time: 10_000,
      effective_end_time: 20_000,
    },
    {
      app_name: "Alma",
      exe_name: "alma.exe",
      window_title: "Alma",
      start_time: 20_000,
      effective_end_time: 30_000,
    },
  ]);

  assert.equal(rows.length, 1);
  assert.equal(rows[0].exeName, "alma.exe");
});

await runTest("aggregate repository mapping prorates partial imported bucket ranges", () => {
  const rows = mapRawAggregateSessionCandidates([
    {
      record_id: 1,
      origin: "import_bucket",
      app_name: "Cursor",
      exe_name: "cursor.exe",
      window_title: "",
      start_time: 0,
      effective_end_time: 60,
      capacity_end_time: 100,
    },
  ], { startTime: 20, endTime: 70 });

  assert.deepEqual(rows, [{
    appName: "Cursor",
    exeName: "cursor.exe",
    startTime: 20,
    endTime: 50,
  }]);
});

await runTest("aggregate repository mapping preserves live native ownership", () => {
  const rows = mapRawAggregateSessionCandidates([{
    record_id: 1,
    origin: "native",
    app_name: "Cursor",
    exe_name: "cursor.exe",
    window_title: "README.md",
    start_time: 10_000,
    effective_end_time: 20_000,
    capacity_end_time: 20_000,
    is_live: 1,
  }]);

  assert.deepEqual(rows, [{
    appName: "Cursor",
    exeName: "cursor.exe",
    startTime: 10_000,
    endTime: 20_000,
    isLive: true,
  }]);
});

await runTest("activity trend clips sessions at range boundaries", () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const rows = buildDataTrendViewModel([
    makeSession({
      startTime: new Date(2026, 4, 7, 23, 0, 0).getTime(),
      endTime: new Date(2026, 4, 8, 1, 0, 0).getTime(),
    }),
  ], 7, nowMs);

  assert.equal(rows.chartData.at(-2)?.hours, 1);
  assert.equal(rows.chartData.at(-1)?.hours, 1);
});

await runTest("app trend respects user exclusions after aggregate DTO tightening", () => {
  ProcessMapper.setUserOverride("cursor.exe", { track: false });
  try {
    const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
    const rows = buildDataAppTrendViewModel([
      makeSession({
        startTime: new Date(2026, 4, 8, 9, 0, 0).getTime(),
        endTime: new Date(2026, 4, 8, 10, 0, 0).getTime(),
      }),
      makeSession({
        appName: "Blender",
        exeName: "blender.exe",
        startTime: new Date(2026, 4, 8, 10, 0, 0).getTime(),
        endTime: new Date(2026, 4, 8, 11, 0, 0).getTime(),
      }),
    ], 7, nowMs, null);

    assert.deepEqual(rows.appOptions.map((app) => app.exeName), ["blender.exe"]);
  } finally {
    ProcessMapper.clearUserOverrides();
  }
});

await runTest("recent heatmap range is aligned to whole local weeks", () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const range = getHeatmapRange("recent", nowMs);

  assert.equal(range.weekCount, 53);
  assert.equal(range.start.getDay(), 1);
  assert.equal(range.end.getDay(), 1);
});

await runTest("heatmap snapshot caches earliest activity and only retains daily totals", async () => {
  resetDataReadModelCacheForTests();
  let earliestLoadCount = 0;
  let sessionLoadCount = 0;
  const sessions = [
    makeSession({
      startTime: new Date(2026, 0, 1, 9, 0, 0).getTime(),
      endTime: new Date(2026, 0, 1, 10, 0, 0).getTime(),
    }),
  ];
  const deps: DataHeatmapDependencies = {
    getEarliestSessionStartTime: async () => {
      earliestLoadCount += 1;
      return sessions[0].startTime;
    },
    getSessionsInRange: async () => {
      sessionLoadCount += 1;
      return sessions;
    },
  };
  const nowMs = new Date(2026, 0, 3, 12, 0, 0).getTime();

  const first = await loadDataHeatmapSnapshot(2026, nowMs, deps);
  const cached = getCachedDataHeatmapDays(2026, nowMs);
  const second = await loadDataHeatmapSnapshot(2026, nowMs, deps);

  assert.equal(first.earliestStartTime, sessions[0].startTime);
  assert.equal(cached, first.days);
  assert.deepEqual(second.days, first.days);
  assert.equal(first.days.find((day) => day.date === "2026-01-01")?.duration, 3_600_000);
  assert.ok(!("sessions" in first));
  assert.equal(earliestLoadCount, 2);
  assert.equal(sessionLoadCount, 2);
});

await runTest("recent heatmap prewarm reuses a warm cache", async () => {
  resetDataReadModelCacheForTests();
  let earliestLoadCount = 0;
  let sessionLoadCount = 0;
  const sessions = [
    makeSession({
      startTime: new Date(2026, 0, 1, 9, 0, 0).getTime(),
      endTime: new Date(2026, 0, 1, 10, 0, 0).getTime(),
    }),
  ];
  const deps: DataHeatmapDependencies = {
    getEarliestSessionStartTime: async () => {
      earliestLoadCount += 1;
      return sessions[0].startTime;
    },
    getSessionsInRange: async () => {
      sessionLoadCount += 1;
      return sessions;
    },
  };
  const nowMs = new Date(2026, 0, 3, 12, 0, 0).getTime();

  const first = await prewarmRecentDataHeatmapCache(nowMs, deps);
  const second = await prewarmRecentDataHeatmapCache(nowMs, deps);

  assert.equal(second.days, first.days);
  assert.equal(first.days.length, 371);
  assert.equal(earliestLoadCount, 1);
  assert.equal(sessionLoadCount, 1);
});

await runTest("heatmap daily cache keeps a small LRU set", async () => {
  resetDataReadModelCacheForTests();
  const deps: DataHeatmapDependencies = {
    getEarliestSessionStartTime: async () => null,
    getSessionsInRange: async () => [],
  };
  const nowMs = new Date(2026, 0, 3, 12, 0, 0).getTime();

  await loadDataHeatmapSnapshot("recent", nowMs, deps);
  await loadDataHeatmapSnapshot(2025, nowMs, deps);
  await loadDataHeatmapSnapshot(2026, nowMs, deps);

  assert.equal(getDataHeatmapDayCacheSizeForTests(), 2);
  assert.equal(getCachedDataHeatmapDays("recent", nowMs), undefined);
});

await runTest("50,000 heatmap records retain only bounded daily values", async () => {
  resetDataReadModelCacheForTests();
  const start = new Date(2026, 0, 1).getTime();
  const nowMs = new Date(2026, 11, 31, 12).getTime();
  const sessions = Array.from({ length: 50_000 }, (_, index) => makeSession({
    startTime: start + index * 10_000,
    endTime: start + index * 10_000 + 1000,
  }));
  const snapshot = await loadDataHeatmapSnapshot(2026, nowMs, {
    getEarliestSessionStartTime: async () => start,
    getSessionsInRange: async () => sessions,
  });
  assert.ok(snapshot.days.length <= 378);
  assert.equal(snapshot.days.reduce((sum, day) => sum + day.duration, 0), 50_000_000);
  assert.ok(snapshot.days.every((day) => Object.keys(day).sort().join() === "date,duration"));
  assert.deepEqual(buildDailyActivityHeatmap(snapshot.days, 2026, nowMs), buildActivityHeatmap(sessions, 2026, nowMs));
  assert.deepEqual(snapshot.days, aggregateHeatmapDays(sessions, 2026, nowMs));
  assert.ok(JSON.stringify(snapshot.days).length < JSON.stringify(sessions).length / 100);
  sessions[0].endTime += 12345;
  assert.equal(snapshot.days.reduce((sum, day) => sum + day.duration, 0), 50_000_000);
});

await runTest("late heatmap reads cannot repopulate a cleared background cache", async () => {
  resetDataReadModelCacheForTests();
  let finish!: (sessions: AggregateSessionRecord[]) => void;
  const nowMs = new Date(2026, 0, 3).getTime();
  const pending = loadDataHeatmapSnapshot("recent", nowMs, {
    getEarliestSessionStartTime: async () => 123,
    getSessionsInRange: () => new Promise((resolve) => { finish = resolve; }),
  });
  clearDataReadModelCache();
  finish([]);
  await pending;
  assert.equal(getDataHeatmapDayCacheSizeForTests(), 0);
  assert.equal(getCachedEarliestSessionStartTime(), undefined);
});

await runTest("heatmap page and prewarm reuse one in-flight read and can retry failures", async () => {
  resetDataReadModelCacheForTests();
  let finish!: (sessions: AggregateSessionRecord[]) => void;
  let loads = 0;
  const nowMs = new Date(2026, 0, 3).getTime();
  const deps: DataHeatmapDependencies = {
    getEarliestSessionStartTime: async () => null,
    getSessionsInRange: () => {
      loads += 1;
      return new Promise((resolve) => { finish = resolve; });
    },
  };
  const first = loadDataHeatmapSnapshot("recent", nowMs, deps);
  const prewarm = prewarmRecentDataHeatmapCache(nowMs, deps);
  assert.equal(loads, 1);
  finish([]);
  const [snapshot, warmed] = await Promise.all([first, prewarm]);
  assert.equal(snapshot, warmed);
  clearDataReadModelCache();
  await assert.rejects(loadDataHeatmapSnapshot("recent", nowMs, {
    ...deps, getSessionsInRange: async () => { throw new Error("read failed"); },
  }), /read failed/);
  const retried = loadDataHeatmapSnapshot("recent", nowMs, deps);
  finish([]);
  await retried;
  assert.equal(loads, 2);
});

await runTest("an obsolete heatmap completion cannot delete a newer pending read", async () => {
  resetDataReadModelCacheForTests();
  let loads = 0;
  const finishes: Array<(sessions: AggregateSessionRecord[]) => void> = [];
  const nowMs = new Date(2026, 0, 3).getTime();
  const deps: DataHeatmapDependencies = {
    getEarliestSessionStartTime: async () => null,
    getSessionsInRange: () => {
      loads += 1;
      return new Promise((resolve) => { finishes.push(resolve); });
    },
  };
  const oldRead = loadDataHeatmapSnapshot("recent", nowMs, deps);
  clearDataReadModelCache();
  const newRead = loadDataHeatmapSnapshot("recent", nowMs, deps);
  finishes[0]([]);
  await oldRead;
  assert.equal(getDataHeatmapDayCacheSizeForTests(), 0);
  assert.equal(loadDataHeatmapSnapshot("recent", nowMs, deps), newRead);
  finishes[1]([]);
  await newRead;
  assert.equal(loads, 2);
  assert.equal(getDataHeatmapDayCacheSizeForTests(), 1);
});

await runTest("data bootstrap snapshot loads a valid persisted payload into cache", async () => {
  const snapshot = makeBootstrapSnapshot();
  const loaded = await loadPersistedDataBootstrapSnapshot({
    loadPayload: async () => JSON.stringify({ ...snapshot, heatmapReadVersion: 2, overviewReadVersion: 1, appReadVersion: 1 }),
    savePayload: async () => {
      throw new Error("unexpected save");
    },
    clearPayload: async () => {
      throw new Error("unexpected clear");
    },
    warn: () => {
      throw new Error("unexpected warning");
    },
  });

  assert.equal(loaded?.createdAtMs, snapshot.createdAtMs);
  assert.equal(loaded?.overviewTrendViewModel.totalDuration, snapshot.overviewTrendViewModel.totalDuration);
});

await runTest("data bootstrap snapshot rejects incomplete app options and clears the payload", async () => {
  const snapshot = makeBootstrapSnapshot();
  const staleOption = snapshot.appTrendViewModel.appOptions[0] as unknown as Record<string, unknown>;
  delete staleOption.exeName;
  let cleared = false;

  const loaded = await loadPersistedDataBootstrapSnapshot({
    loadPayload: async () => JSON.stringify({ ...snapshot, heatmapReadVersion: 2, overviewReadVersion: 1, appReadVersion: 1 }),
    savePayload: async () => {
      throw new Error("unexpected save");
    },
    clearPayload: async () => {
      cleared = true;
    },
    warn: () => {
      throw new Error("unexpected warning");
    },
  });

  assert.equal(loaded, null);
  assert.equal(cleared, true);
});

await runTest("data bootstrap snapshot refuses oversized payloads", async () => {
  const warnings: string[] = [];
  let saved = false;
  const snapshot = makeBootstrapSnapshot({
    heatmapRows: Array.from({ length: 12_000 }, (_, index) => ({
      key: `week-${index}`,
      monthLabel: "5月",
      cells: [],
    })),
  });

  const didSave = await saveDataBootstrapSnapshot(snapshot, { minSaveIntervalMs: 0 }, {
    loadPayload: async () => null,
    savePayload: async () => {
      saved = true;
    },
    clearPayload: async () => undefined,
    warn: (message) => warnings.push(message),
  });

  assert.equal(didSave, false);
  assert.equal(saved, false);
  assert.equal(warnings.length, 1);
});

await runTest("data first screen prewarm saves a bootstrap snapshot", async () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const sessions = [
    makeSession({
      startTime: new Date(2026, 4, 8, 9, 0, 0).getTime(),
      endTime: new Date(2026, 4, 8, 10, 0, 0).getTime(),
    }),
  ];
  const trendSnapshot = await loadDataTrendSnapshot({ kind: "rolling", days: 7 }, nowMs, {
    getDailyApps: async () => dailyAppsFixture(sessions),
  });
  let savedSnapshot: DataBootstrapSnapshot | null = null;

  const snapshot = await prewarmDataFirstScreen({
    mappingVersion: 3,
    reason: "data-opened",
    uiLanguage: "zh-CN",
    nowMs,
  }, {
    loadTrendSnapshot: async () => trendSnapshot,
    prewarmRecentHeatmap: async () => ({
      earliestStartTime: sessions[0].startTime,
      range: getHeatmapRange("recent", nowMs),
      cacheKey: "recent:2025-05-05:2026-05-11",
      days: aggregateHeatmapDays(sessions, "recent", nowMs),
    }),
    saveBootstrapSnapshot: async (nextSnapshot) => {
      savedSnapshot = nextSnapshot;
      return true;
    },
    warn: () => {
      throw new Error("unexpected warning");
    },
  });

  assert.equal(snapshot?.mappingVersion, 3);
  assert.equal(savedSnapshot?.overviewTrendViewModel.totalDuration, 60 * 60 * 1000);
  assert.equal(savedSnapshot?.appTrendViewModel.selectedApp?.appName, "Cursor");
  assert.ok(savedSnapshot?.heatmapRows.length);
});

await runTest("data first screen prewarm dedupes pending matching work and throttles repeats", async () => {
  const nowMs = new Date(2026, 4, 8, 12, 0, 0).getTime();
  const sessions = [
    makeSession({
      startTime: new Date(2026, 4, 8, 9, 0, 0).getTime(),
      endTime: new Date(2026, 4, 8, 10, 0, 0).getTime(),
    }),
  ];
  const trendSnapshot = await loadDataTrendSnapshot({ kind: "rolling", days: 7 }, nowMs, {
    getDailyApps: async () => dailyAppsFixture(sessions),
  });
  let loadCount = 0;
  let releaseLoad: (() => void) | null = null;
  const deps = {
    loadTrendSnapshot: async (): Promise<DataTrendSnapshot> => {
      loadCount += 1;
      await new Promise<void>((resolve) => {
        releaseLoad = resolve;
      });
      return trendSnapshot;
    },
    prewarmRecentHeatmap: async () => ({
      earliestStartTime: sessions[0].startTime,
      range: getHeatmapRange("recent", nowMs),
      cacheKey: "recent:2025-05-05:2026-05-11",
      days: aggregateHeatmapDays(sessions, "recent", nowMs),
    }),
    saveBootstrapSnapshot: async () => true,
    warn: () => {
      throw new Error("unexpected warning");
    },
  };

  const first = prewarmDataFirstScreen({
    mappingVersion: 1,
    reason: "data-opened",
    uiLanguage: "zh-CN",
    nowMs,
  }, deps);
  const second = prewarmDataFirstScreen({
    mappingVersion: 1,
    reason: "data-opened",
    uiLanguage: "zh-CN",
    nowMs,
  }, deps);
  releaseLoad?.();
  await Promise.all([first, second]);

  const throttled = await prewarmDataFirstScreen({
    mappingVersion: 1,
    reason: "data-opened",
    uiLanguage: "zh-CN",
    nowMs: nowMs + 1_000,
  }, deps);

  assert.equal(loadCount, 1);
  assert.equal(throttled, null);
});

await runTest("data heavy cache cleanup clears trend and heatmap caches without bootstrap", async () => {
  resetDataReadModelCacheForTests();
  const nowMs = new Date(2026, 0, 3, 12, 0, 0).getTime();
  await loadDataTrendSnapshot({ kind: "rolling", days: 7 }, nowMs, {
    getDailyApps: async () => dailyAppsFixture([]),
  });
  await loadDataHeatmapSnapshot("recent", nowMs, {
    getEarliestSessionStartTime: async () => null,
    getSessionsInRange: async () => [],
  });
  await saveDataBootstrapSnapshot(makeBootstrapSnapshot(), { minSaveIntervalMs: 0 }, {
    clearPayload: async () => undefined,
    loadPayload: async () => null,
    savePayload: async () => undefined,
  });

  assert.equal(getDataTrendSnapshotCacheSizeForTests(), 1);
  assert.equal(getDataHeatmapDayCacheSizeForTests(), 1);

  clearDataHeavyCaches();

  assert.equal(getDataTrendSnapshotCacheSizeForTests(), 0);
  assert.equal(getDataHeatmapDayCacheSizeForTests(), 0);
  assert.equal((await loadPersistedDataBootstrapSnapshot({
    clearPayload: async () => undefined,
    loadPayload: async () => JSON.stringify({ ...makeBootstrapSnapshot(), heatmapReadVersion: 2, overviewReadVersion: 1, appReadVersion: 1 }),
    savePayload: async () => undefined,
  }))?.overviewRangeCacheKey, "rolling:7:2026-05-02:2026-05-08");
});

console.log(`Passed ${passed} data read model tests`);

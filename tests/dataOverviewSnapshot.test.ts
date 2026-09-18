import assert from "node:assert/strict";
import { buildDailyDataTrendViewModel, buildDataTrendViewModel } from "../src/features/data/services/dataReadModel.ts";
import { resolveDataTrendRange } from "../src/features/data/services/dataTrendRange.ts";
import { clearDataHeavyCaches } from "../src/features/data/services/dataCacheLifecycle.ts";
import { clearDataOverviewSnapshotCache, getCachedDataOverviewSnapshot, loadDataOverviewSnapshot } from "../src/features/data/services/dataOverviewSnapshot.ts";
import { getDailyActivity, type DailyActivityRead } from "../src/platform/persistence/dailyActivityRepository.ts";

let passed = 0;
async function test(name: string, run: () => void | Promise<void>) {
  clearDataOverviewSnapshotCache();
  await run();
  console.log(`PASS ${name}`);
  passed++;
}
const now = new Date(2026, 4, 20, 12).getTime();
const selection = { kind: "rolling", days: 7 } as const;
const empty = { earliestStartTime: null, days: [] };

await test("daily totals preserve chart labels, averaging and drill-down dates", () => {
  const sessions = [
    { appName: "Cursor", exeName: "cursor.exe", startTime: new Date(2026, 3, 30, 23).getTime(), endTime: new Date(2026, 4, 1, 1).getTime() },
    { appName: "Cursor", exeName: "cursor.exe", startTime: now - 37, endTime: now },
  ];
  const days = [
    { date: "2026-04-30", duration: 3600000 },
    { date: "2026-05-01", duration: 3600000 },
    { date: "2026-05-20", duration: 37 },
  ];
  for (const daysCount of [7, 30, 365] as const) {
    const range = resolveDataTrendRange({ kind: "rolling", days: daysCount }, now);
    assert.deepEqual(buildDailyDataTrendViewModel(days, range), buildDataTrendViewModel(sessions, range, now));
  }
  const midnight = new Date(2026, 4, 20).getTime();
  const range = resolveDataTrendRange(selection, midnight);
  assert.equal(buildDailyDataTrendViewModel(days, range).totalDuration, 0);
});

await test("overview requests full local day boundaries and deduplicates reads", async () => {
  let calls = 0;
  const read: typeof getDailyActivity = async (start, end) => {
    calls++;
    assert.equal(start, new Date(2026, 4, 14).getTime());
    assert.equal(end, new Date(2026, 4, 21).getTime());
    return { earliestStartTime: null, days: [{ date: "2026-05-20", duration: 37 }] };
  };
  const [a, b] = await Promise.all([loadDataOverviewSnapshot(selection, now, read), loadDataOverviewSnapshot(selection, now, read)]);
  assert.equal(calls, 1);
  assert.equal(a.days, b.days);
  assert.equal("sessions" in a, false);
  assert.equal(getCachedDataOverviewSnapshot(a.range)?.days, a.days);
  clearDataHeavyCaches();
  assert.equal(getCachedDataOverviewSnapshot(a.range), null);
});

await test("invalidated requests cannot refill cache or evict replacement requests", async () => {
  let finishOld!: (value: DailyActivityRead) => void;
  let finishNew!: (value: DailyActivityRead) => void;
  const old = loadDataOverviewSnapshot(selection, now, () => new Promise(resolve => { finishOld = resolve; }));
  clearDataOverviewSnapshotCache();
  const next = loadDataOverviewSnapshot(selection, now, () => new Promise(resolve => { finishNew = resolve; }));
  finishOld(empty);
  await old;
  const range = resolveDataTrendRange(selection, now);
  assert.equal(getCachedDataOverviewSnapshot(range), null);
  const joined = loadDataOverviewSnapshot(selection, now, async () => { throw new Error("must deduplicate"); });
  finishNew({ ...empty, days: [{ date: "2026-05-20", duration: 1 }] });
  await Promise.all([next, joined]);
  assert.equal(getCachedDataOverviewSnapshot(range)?.days[0].duration, 1);
});

await test("failed reads can retry and oversize ranges never reach persistence", async () => {
  await assert.rejects(loadDataOverviewSnapshot(selection, now, async () => { throw new Error("busy"); }), /busy/);
  await loadDataOverviewSnapshot(selection, now, async () => empty);
  await assert.rejects(loadDataOverviewSnapshot({ kind: "custom", startDateKey: "2020-01-01", endDateKey: "2026-05-20" }, now,
    async () => { assert.fail("oversized query reached persistence"); }), /overview-range-limit/);
});

await test("overview retains at most four recently used snapshots", async () => {
  const ranges = [];
  for (let day = 1; day <= 5; day++) {
    ranges.push((await loadDataOverviewSnapshot({ kind: "custom", startDateKey: `2026-05-0${day}`, endDateKey: `2026-05-0${day}` }, now, async () => empty)).range);
  }
  assert.equal(getCachedDataOverviewSnapshot(ranges[0]), null);
  assert.notEqual(getCachedDataOverviewSnapshot(ranges[1]), null);
});

await test("desktop daily reads serialize and a rejected request releases the queue", async () => {
  const start = new Date(2026, 4, 19).getTime();
  const end = new Date(2026, 4, 20).getTime();
  let rejectFirst!: (reason: Error) => void;
  let secondStarted = false;
  const first = getDailyActivity(start, end, () => new Promise((_resolve, reject) => { rejectFirst = reject; }));
  const rejected = assert.rejects(first, /busy/);
  const second = getDailyActivity(start, end, async () => {
    secondStarted = true;
    return { sampled_at_ms: now, earliest_start_ms: null, days: [{ start_ms: start, end_ms: end, active_ms: 37 }] };
  });
  await Promise.resolve();
  assert.equal(secondStarted, false);
  rejectFirst(new Error("busy"));
  await rejected;
  assert.deepEqual((await second).days, [{ date: "2026-05-19", duration: 37 }]);
});

console.log(`\n${passed} overview tests passed.`);

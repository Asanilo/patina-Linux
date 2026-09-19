import assert from "node:assert/strict";
import { dailyAppsFixture } from "./helpers/dailyAppsFixture.ts";
import { buildDailyAppTrendViewModel, buildDailyCategoryTrendViewModel } from "../src/features/data/services/dataDailyAppReadModel.ts";
import { buildDataAppTrendViewModel } from "../src/features/data/services/dataReadModel.ts";
import { buildDataCategoryTrendViewModel } from "../src/features/data/services/dataCategoryTrendReadModel.ts";
import { resolveDataTrendRange } from "../src/features/data/services/dataTrendRange.ts";
import { ProcessMapper } from "../src/shared/classification/processMapper.ts";

const now = new Date(2026, 4, 8, 18).getTime();
const sessions = [
  { exeName: "Alpha.EXE", appName: "Alpha", startTime: new Date(2026, 3, 30, 23).getTime(), endTime: new Date(2026, 4, 1, 1).getTime() },
  { exeName: "alpha.exe", appName: "Alpha", startTime: now - 37, endTime: now },
  { exeName: "chat.exe", appName: "Chat", startTime: now - 60000, endTime: now - 10000 },
  { exeName: "steamwebhelper.exe", appName: "Helper", startTime: now - 10000, endTime: now - 5000 },
];
ProcessMapper.setUserOverrides({ "alpha.exe": { category: "development", displayName: "Research" }, "chat.exe": { category: "communication" } });
for (const days of [7, 30, 365] as const) {
  const range = resolveDataTrendRange({ kind: "rolling", days }, now);
  const activity = dailyAppsFixture(sessions.filter(session => session.endTime > range.startMs && session.startTime < range.endMs));
  for (const selected of [null, "alpha.exe", "chat.exe"]) {
    assert.deepEqual(buildDailyAppTrendViewModel(activity, range, selected), buildDataAppTrendViewModel(sessions, range, now, selected));
  }
  for (const selected of [[], ["development"], ["development", "communication"]]) {
    assert.deepEqual(buildDailyCategoryTrendViewModel(activity, range, selected), buildDataCategoryTrendViewModel(sessions, range, now, selected));
  }
  const emptyFirst = buildDailyCategoryTrendViewModel(activity, range, ["design", "development"]);
  assert.ok(emptyFirst.chartRows.every(row => row.series0 === 0));
  assert.ok(emptyFirst.chartRows.some(row => Number(row.series1) > 0));
}
ProcessMapper.setUserOverrides({ "alpha.exe": { track: false } });
const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, now);
assert.equal(buildDailyAppTrendViewModel(dailyAppsFixture(sessions), range, null).appOptions.some(app => app.appKey === "alpha.exe"), false);
assert.equal(buildDailyAppTrendViewModel(dailyAppsFixture([]), range, null).selectedApp, null);
ProcessMapper.clearUserOverrides();
console.log("PASS daily application/category parity, local midnight/month, names, selection, exclusions and exact millisecond totals");

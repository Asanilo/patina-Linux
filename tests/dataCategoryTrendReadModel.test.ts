import assert from "node:assert/strict";
import {
  buildDataCategoryTrendViewModel,
  filterDataCategoryOptionsForQuery,
} from "../src/features/data/services/dataCategoryTrendReadModel.ts";
import type { AggregateSessionRecord } from "../src/features/data/services/dataReadModel.ts";
import { resolveDataTrendRange } from "../src/features/data/services/dataTrendRange.ts";
import { ProcessMapper } from "../src/shared/classification/processMapper.ts";

const HOUR = 3_600_000;
let passed = 0;

function makeSession(
  exeName: string,
  appName: string,
  startTime: number,
  duration: number,
): AggregateSessionRecord {
  return {
    exeName,
    appName,
    startTime,
    endTime: startTime + duration,
  };
}

async function runTest(name: string, fn: () => void | Promise<void>) {
  ProcessMapper.clearUserOverrides();
  try {
    await fn();
    passed += 1;
    console.log(`PASS ${name}`);
  } finally {
    ProcessMapper.clearUserOverrides();
  }
}

await runTest("category trend conserves grouped durations and merges executable casing", () => {
  ProcessMapper.setUserOverrides({
    "alpha.exe": { category: "development" },
    "chat.exe": { category: "communication" },
  });
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const sessions = [
    makeSession("Alpha.EXE", "Alpha", new Date(2026, 4, 6, 9).getTime(), HOUR),
    makeSession("alpha.exe", "Alpha", new Date(2026, 4, 7, 9).getTime(), 2 * HOUR),
    makeSession("chat.exe", "Chat", new Date(2026, 4, 8, 9).getTime(), HOUR / 2),
  ];
  const viewModel = buildDataCategoryTrendViewModel(
    sessions,
    range,
    nowMs,
    ["development"],
  );
  const development = viewModel.categoryOptions.find(
    (option) => option.category === "development",
  );

  assert.ok(development);
  assert.equal(development.appCount, 1);
  assert.equal(development.totalDuration, 3 * HOUR);
  assert.equal(viewModel.summary.totalDuration, 3 * HOUR);
  assert.equal(viewModel.summary.activeDayCount, 2);
  assert.equal(
    viewModel.categoryOptions.reduce((sum, option) => sum + option.totalDuration, 0),
    3.5 * HOUR,
  );
});

await runTest("category trend combines multi-selection metrics and series", () => {
  ProcessMapper.setUserOverrides({
    "alpha.exe": { category: "development" },
    "chat.exe": { category: "communication" },
  });
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const viewModel = buildDataCategoryTrendViewModel([
    makeSession("alpha.exe", "Alpha", new Date(2026, 4, 6, 9).getTime(), HOUR),
    makeSession("chat.exe", "Chat", new Date(2026, 4, 6, 11).getTime(), 2 * HOUR),
    makeSession("chat.exe", "Chat", new Date(2026, 4, 8, 9).getTime(), HOUR),
  ], range, nowMs, ["development", "communication"]);

  assert.equal(viewModel.chartSeries.length, 2);
  assert.equal(viewModel.summary.totalDuration, 4 * HOUR);
  assert.equal(viewModel.summary.activeDayCount, 2);
  assert.equal(viewModel.peakDay?.duration, 3 * HOUR);
  assert.equal(
    viewModel.chartRows.reduce((sum, row) => sum + row.totalDuration, 0),
    4 * HOUR,
  );
});

await runTest("category trend splits cross-midnight activity at local day boundaries", () => {
  ProcessMapper.setUserOverrides({
    "alpha.exe": { category: "development" },
  });
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const viewModel = buildDataCategoryTrendViewModel([
    makeSession("alpha.exe", "Alpha", new Date(2026, 4, 6, 23).getTime(), 2 * HOUR),
  ], range, nowMs, ["development"]);
  const activeRows = viewModel.chartRows.filter((row) => row.totalDuration > 0);

  assert.deepEqual(activeRows.map((row) => row.totalDuration), [HOUR, HOUR]);
  assert.equal(viewModel.summary.activeDayCount, 2);
});

await runTest("category search changes visible options without changing statistics", () => {
  ProcessMapper.setUserOverrides({
    "alpha.exe": { category: "development" },
    "chat.exe": { category: "communication" },
  });
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const viewModel = buildDataCategoryTrendViewModel([
    makeSession("alpha.exe", "Alpha", new Date(2026, 4, 7, 9).getTime(), HOUR),
    makeSession("chat.exe", "Chat", new Date(2026, 4, 8, 9).getTime(), 2 * HOUR),
  ], range, nowMs, ["development"]);
  const before = viewModel.categoryOptions.reduce(
    (sum, option) => sum + option.totalDuration,
    0,
  );
  const filtered = filterDataCategoryOptionsForQuery(
    viewModel.categoryOptions,
    viewModel.selectedCategories[0]?.displayName ?? "",
  );

  assert.deepEqual(filtered.map((option) => option.category), ["development"]);
  assert.equal(
    viewModel.categoryOptions.reduce((sum, option) => sum + option.totalDuration, 0),
    before,
  );
});

await runTest("category trend preserves a valid empty-range selection", () => {
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const viewModel = buildDataCategoryTrendViewModel(
    [],
    range,
    nowMs,
    ["development"],
  );

  assert.equal(viewModel.categoryOptions.length, 0);
  assert.equal(viewModel.selectedCategories[0]?.category, "development");
  assert.equal(viewModel.summary.totalDuration, 0);
  assert.equal(viewModel.chartSeries.length, 1);
});

console.log(`Passed ${passed} data category trend read-model tests`);

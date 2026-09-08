import assert from "node:assert/strict";
import {
  buildDataWebActivityTrendViewModel,
  filterDataWebDomainOptionsForQuery,
} from "../src/features/data/services/dataWebActivityReadModel.ts";
import {
  clearDataWebActivitySnapshotCache,
  getDataWebActivitySnapshotCacheSizeForTests,
  loadDataWebActivitySnapshot,
} from "../src/features/data/services/dataWebActivitySnapshot.ts";
import { resolveDataTrendRange } from "../src/features/data/services/dataTrendRange.ts";
import type { WebActivityTrendSegment } from "../src/shared/types/webActivity.ts";

const HOUR = 3_600_000;
let passed = 0;

function makeSegment(
  normalizedDomain: string,
  startTime: number,
  duration: number,
  overrides: Partial<WebActivityTrendSegment> = {},
): WebActivityTrendSegment {
  return {
    id: Math.round(startTime / 1000),
    browserClientId: "profile-a",
    browserKind: "firefox",
    browserExeName: "zen",
    domain: normalizedDomain,
    normalizedDomain,
    faviconUrl: null,
    startTime,
    endTime: startTime + duration,
    ...overrides,
  };
}

async function runTest(name: string, fn: () => void | Promise<void>) {
  clearDataWebActivitySnapshotCache();
  await fn();
  passed += 1;
  console.log(`PASS ${name}`);
}

await runTest("web trend groups domains and splits cross-midnight activity", () => {
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const viewModel = buildDataWebActivityTrendViewModel([
    makeSegment("docs.example.com", new Date(2026, 4, 6, 23).getTime(), 2 * HOUR),
    makeSegment("chat.example.com", new Date(2026, 4, 8, 9).getTime(), HOUR),
  ], {}, range, nowMs, ["docs.example.com"]);
  const activeRows = viewModel.chartRows.filter((row) => row.totalDuration > 0);

  assert.deepEqual(activeRows.map((row) => row.totalDuration), [HOUR, HOUR]);
  assert.equal(viewModel.domainOptions.length, 2);
  assert.equal(viewModel.summary.totalDuration, 2 * HOUR);
  assert.equal(viewModel.summary.activeDayCount, 2);
});

await runTest("web trend unions overlapping intervals per browser source", () => {
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const start = new Date(2026, 4, 8, 9).getTime();
  const viewModel = buildDataWebActivityTrendViewModel([
    makeSegment("docs.example.com", start, HOUR),
    makeSegment("docs.example.com", start + (HOUR / 2), HOUR),
    makeSegment("docs.example.com", start, HOUR, { browserClientId: "profile-b" }),
  ], {}, range, nowMs, ["docs.example.com"]);

  assert.equal(viewModel.summary.totalDuration, 2.5 * HOUR);
});

await runTest("web trend applies display category color and exclusion overrides", () => {
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const viewModel = buildDataWebActivityTrendViewModel([
    makeSegment("docs.example.com", new Date(2026, 4, 7, 9).getTime(), HOUR),
    makeSegment("blocked.example.com", new Date(2026, 4, 7, 10).getTime(), HOUR),
  ], {
    "docs.example.com": {
      displayName: "Documentation",
      category: "development",
      color: "#123456",
    },
    "blocked.example.com": { enabled: false },
  }, range, nowMs, ["docs.example.com"]);

  assert.equal(viewModel.domainOptions.length, 1);
  assert.equal(viewModel.domainOptions[0]?.displayName, "Documentation");
  assert.equal(viewModel.domainOptions[0]?.category, "development");
  assert.equal(viewModel.domainOptions[0]?.color, "#123456");
});

await runTest("web trend combines selected domains without exposing page-level fields", () => {
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const viewModel = buildDataWebActivityTrendViewModel([
    makeSegment("docs.example.com", new Date(2026, 4, 6, 9).getTime(), HOUR),
    makeSegment("chat.example.com", new Date(2026, 4, 6, 11).getTime(), 2 * HOUR),
  ], {}, range, nowMs, ["docs.example.com", "chat.example.com"]);

  assert.equal(viewModel.chartSeries.length, 2);
  assert.equal(viewModel.summary.totalDuration, 3 * HOUR);
  assert.equal(viewModel.peakDay?.duration, 3 * HOUR);
  assert.equal("url" in viewModel.domainOptions[0]!, false);
  assert.equal("title" in viewModel.domainOptions[0]!, false);
});

await runTest("web search matches display names and normalized domains", () => {
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  const range = resolveDataTrendRange({ kind: "rolling", days: 7 }, nowMs);
  const viewModel = buildDataWebActivityTrendViewModel([
    makeSegment("docs.example.com", new Date(2026, 4, 6, 9).getTime(), HOUR),
  ], {
    "docs.example.com": { displayName: "Documentation" },
  }, range, nowMs, []);

  assert.equal(filterDataWebDomainOptionsForQuery(viewModel.domainOptions, "doc").length, 1);
  assert.equal(filterDataWebDomainOptionsForQuery(viewModel.domainOptions, "example.com").length, 1);
  assert.equal(filterDataWebDomainOptionsForQuery(viewModel.domainOptions, "chat").length, 0);
});

await runTest("web snapshot dedupes range reads and tolerates unavailable overrides", async () => {
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  let segmentLoads = 0;
  let overrideLoads = 0;
  const deps = {
    getSegmentsInRange: async () => {
      segmentLoads += 1;
      return [makeSegment("docs.example.com", nowMs - HOUR, HOUR)];
    },
    loadOverrides: async () => {
      overrideLoads += 1;
      throw new Error("optional settings unavailable");
    },
  };
  const selection = { kind: "rolling", days: 7 } as const;
  const [left, right] = await Promise.all([
    loadDataWebActivitySnapshot(selection, nowMs, "mapping:1", deps),
    loadDataWebActivitySnapshot(selection, nowMs, "mapping:1", deps),
  ]);

  assert.equal(segmentLoads, 1);
  assert.equal(overrideLoads, 1);
  assert.equal(left.segments.length, 1);
  assert.deepEqual(right.overrides, {});
  assert.equal(getDataWebActivitySnapshotCacheSizeForTests(), 1);
});

await runTest("web trend snapshot cache supports explicit cleanup", async () => {
  const nowMs = new Date(2026, 4, 8, 18).getTime();
  await loadDataWebActivitySnapshot(
    { kind: "rolling", days: 7 },
    nowMs,
    "mapping:1",
    {
      getSegmentsInRange: async () => [],
      loadOverrides: async () => ({}),
    },
  );
  assert.equal(getDataWebActivitySnapshotCacheSizeForTests(), 1);

  clearDataWebActivitySnapshotCache();
  assert.equal(getDataWebActivitySnapshotCacheSizeForTests(), 0);
});

console.log(`Passed ${passed} data web activity read-model tests`);

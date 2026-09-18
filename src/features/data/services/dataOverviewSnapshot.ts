import type { getDailyActivity } from "../../../platform/persistence/dailyActivityRepository.ts";
import { resolveDataTrendRange, addLocalDays, parseLocalDateKey, type DataTrendRangeSelection, type ResolvedDataTrendRange } from "./dataTrendRange.ts";
import type { HeatmapDayTotal } from "./dataReadModel.ts";

export interface DataOverviewSnapshot {
  fetchedAtMs: number;
  range: ResolvedDataTrendRange;
  days: HeatmapDayTotal[];
}

const cache = new Map<string, DataOverviewSnapshot>();
const pending = new Map<string, Promise<DataOverviewSnapshot>>();
let generation = 0;

const readDailyActivity: typeof getDailyActivity = async (start, end) => {
  const { getDailyActivity } = await import("../../../platform/persistence/dailyActivityRepository.ts");
  return getDailyActivity(start, end);
};

function remember(snapshot: DataOverviewSnapshot) {
  cache.delete(snapshot.range.cacheKey);
  cache.set(snapshot.range.cacheKey, snapshot);
  while (cache.size > 4) cache.delete(cache.keys().next().value!);
}

export function getCachedDataOverviewSnapshot(range: ResolvedDataTrendRange): DataOverviewSnapshot | null {
  const snapshot = cache.get(range.cacheKey);
  if (!snapshot) return null;
  remember(snapshot);
  return { ...snapshot, range };
}

export function clearDataOverviewSnapshotCache() {
  generation += 1;
  cache.clear();
  pending.clear();
}

export function loadDataOverviewSnapshot(
  selection: DataTrendRangeSelection,
  nowMs = Date.now(),
  read: typeof getDailyActivity = readDailyActivity,
): Promise<DataOverviewSnapshot> {
  const range = resolveDataTrendRange(selection, nowMs);
  if (range.dayCount < 1 || range.dayCount > 378) return Promise.reject(new Error("overview-range-limit"));
  const existing = pending.get(range.cacheKey);
  if (existing) return existing.then(snapshot => ({ ...snapshot, range }));
  const capturedGeneration = generation;
  const end = addLocalDays(parseLocalDateKey(range.endDateKey)!, 1).getTime();
  const request = read(range.startMs, end).then(({ days }) => {
    const snapshot = { fetchedAtMs: nowMs, range, days };
    if (generation === capturedGeneration) remember(snapshot);
    return snapshot;
  }).finally(() => {
    if (pending.get(range.cacheKey) === request) pending.delete(range.cacheKey);
  });
  pending.set(range.cacheKey, request);
  return request;
}

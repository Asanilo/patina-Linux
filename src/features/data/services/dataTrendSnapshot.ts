import type { DailyAppsRead } from "../../../platform/persistence/dailyAppsRepository.ts";
import {
  resolveDataTrendRange,
  addLocalDays,
  parseLocalDateKey,
  type DataTrendRangeSelection,
  type ResolvedDataTrendRange,
} from "./dataTrendRange.ts";

export interface DataTrendSnapshot {
  fetchedAtMs: number;
  range: ResolvedDataTrendRange;
  activity: DailyAppsRead;
}

export interface DataTrendSnapshotDependencies {
  getDailyApps: (startMs: number, endMs: number) => Promise<DailyAppsRead>;
}

const snapshotCache = new Map<string, DataTrendSnapshot>();
const pendingReads = new Map<string, Promise<DailyAppsRead>>();
const DATA_TREND_SNAPSHOT_CACHE_LIMIT = 4;
let cacheGeneration = 0;

function touchSnapshotCacheEntry(key: string, snapshot: DataTrendSnapshot): void {
  snapshotCache.delete(key);
  snapshotCache.set(key, snapshot);

  while (snapshotCache.size > DATA_TREND_SNAPSHOT_CACHE_LIMIT) {
    const oldestKey = snapshotCache.keys().next().value;
    if (!oldestKey) break;
    snapshotCache.delete(oldestKey);
  }
}

export function getCachedDataTrendSnapshot(range: ResolvedDataTrendRange): DataTrendSnapshot | null {
  const snapshot = snapshotCache.get(range.cacheKey);
  if (!snapshot) return null;

  touchSnapshotCacheEntry(range.cacheKey, snapshot);
  return { ...snapshot, range };
}

export function setDataTrendSnapshotCache(snapshot: DataTrendSnapshot): void {
  touchSnapshotCacheEntry(snapshot.range.cacheKey, snapshot);
}

export function clearDataTrendSnapshotCache(): void {
  cacheGeneration += 1;
  snapshotCache.clear();
  pendingReads.clear();
}

export async function loadDataTrendSnapshot(
  selection: DataTrendRangeSelection,
  nowMs: number = Date.now(),
  deps: DataTrendSnapshotDependencies = {
    getDailyApps: async (start, end) => {
      const { getDailyApps } = await import("../../../platform/persistence/dailyAppsRepository.ts");
      return getDailyApps(start, end);
    }
  },
): Promise<DataTrendSnapshot> {
  const range = resolveDataTrendRange(selection, nowMs);
  if (range.dayCount < 1 || range.dayCount > 378) throw new Error("overview-range-limit");
  const generation = cacheGeneration;
  const pending = pendingReads.get(range.cacheKey);
  const read = pending ?? deps.getDailyApps(range.startMs, addLocalDays(parseLocalDateKey(range.endDateKey)!, 1).getTime()).finally(() => {
    if (pendingReads.get(range.cacheKey) === read) pendingReads.delete(range.cacheKey);
  });
  if (!pending) pendingReads.set(range.cacheKey, read);
  return read.then((activity) => {
    const snapshot = { fetchedAtMs: nowMs, range, activity };
    if (generation === cacheGeneration) setDataTrendSnapshotCache(snapshot);
    return snapshot;
  });
}

export function prewarmDefaultDataTrendSnapshot(nowMs: number = Date.now()) {
  return loadDataTrendSnapshot({ kind: "rolling", days: 7 }, nowMs);
}

export function getDataTrendSnapshotCacheSizeForTests(): number {
  return snapshotCache.size;
}

import { getSessionSummariesInRange } from "../../../platform/persistence/sessionReadRepository.ts";
import type { AggregateSessionRecord } from "../../../platform/persistence/sessionReadRepository.ts";
import {
  resolveDataTrendRange,
  type DataTrendRangeSelection,
  type ResolvedDataTrendRange,
} from "./dataTrendRange.ts";

export interface DataTrendSnapshot {
  fetchedAtMs: number;
  range: ResolvedDataTrendRange;
  sessions: AggregateSessionRecord[];
}

export interface DataTrendSnapshotDependencies {
  getSessionSummariesInRange: (startMs: number, endMs: number) => Promise<AggregateSessionRecord[]>;
}

const snapshotCache = new Map<string, DataTrendSnapshot>();
const sessionPromises = new Map<string, Promise<AggregateSessionRecord[]>>();
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
  sessionPromises.clear();
}

export async function loadDataTrendSnapshot(
  selection: DataTrendRangeSelection,
  nowMs: number = Date.now(),
  deps: DataTrendSnapshotDependencies = { getSessionSummariesInRange },
): Promise<DataTrendSnapshot> {
  const range = resolveDataTrendRange(selection, nowMs);
  const generation = cacheGeneration;
  const pending = sessionPromises.get(range.cacheKey);
  const sessionPromise = pending ?? deps.getSessionSummariesInRange(range.startMs, range.endMs).finally(() => {
    if (sessionPromises.get(range.cacheKey) === sessionPromise) sessionPromises.delete(range.cacheKey);
  });
  if (!pending) sessionPromises.set(range.cacheKey, sessionPromise);
  return sessionPromise.then((sessions) => {
    const snapshot = { fetchedAtMs: nowMs, range, sessions };
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

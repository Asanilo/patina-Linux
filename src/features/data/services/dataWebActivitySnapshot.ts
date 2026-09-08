import { getWebActivityTrendSegmentsInRange } from "../../../platform/persistence/dataWebActivityTrendRepository.ts";
import { loadWebDomainOverrides } from "../../../platform/persistence/webActivityRepository.ts";
import type {
  WebActivityTrendSegment,
  WebDomainOverride,
} from "../../../shared/types/webActivity.ts";
import {
  resolveDataTrendRange,
  type DataTrendRangeSelection,
  type ResolvedDataTrendRange,
} from "./dataTrendRange.ts";

export interface DataWebActivitySnapshot {
  fetchedAtMs: number;
  range: ResolvedDataTrendRange;
  segments: WebActivityTrendSegment[];
  overrides: Record<string, WebDomainOverride>;
}

export interface DataWebActivitySnapshotDependencies {
  getSegmentsInRange: (
    startMs: number,
    endMs: number,
  ) => Promise<WebActivityTrendSegment[]>;
  loadOverrides: () => Promise<Record<string, WebDomainOverride>>;
}

const DEFAULT_DEPENDENCIES: DataWebActivitySnapshotDependencies = {
  getSegmentsInRange: getWebActivityTrendSegmentsInRange,
  loadOverrides: loadWebDomainOverrides,
};
const SNAPSHOT_CACHE_LIMIT = 4;
const snapshotCache = new Map<string, DataWebActivitySnapshot>();
const snapshotPromises = new Map<string, Promise<DataWebActivitySnapshot>>();

function getCacheKey(range: ResolvedDataTrendRange, cacheVersion: string) {
  return `${cacheVersion}:${range.cacheKey}`;
}

function touchSnapshot(key: string, snapshot: DataWebActivitySnapshot) {
  snapshotCache.delete(key);
  snapshotCache.set(key, snapshot);
  while (snapshotCache.size > SNAPSHOT_CACHE_LIMIT) {
    const oldestKey = snapshotCache.keys().next().value;
    if (!oldestKey) break;
    snapshotCache.delete(oldestKey);
  }
}

export function getCachedDataWebActivitySnapshot(
  range: ResolvedDataTrendRange,
  cacheVersion: string,
) {
  const key = getCacheKey(range, cacheVersion);
  const snapshot = snapshotCache.get(key);
  if (!snapshot) return null;
  touchSnapshot(key, snapshot);
  return { ...snapshot, range };
}

export async function loadDataWebActivitySnapshot(
  selection: DataTrendRangeSelection,
  nowMs: number = Date.now(),
  cacheVersion: string = "default",
  deps: DataWebActivitySnapshotDependencies = DEFAULT_DEPENDENCIES,
): Promise<DataWebActivitySnapshot> {
  const range = resolveDataTrendRange(selection, nowMs);
  const key = getCacheKey(range, cacheVersion);
  const pending = snapshotPromises.get(key);
  if (pending) return pending;

  const promise = Promise.all([
    deps.getSegmentsInRange(range.startMs, range.endMs),
    deps.loadOverrides().catch((): Record<string, WebDomainOverride> => ({})),
  ]).then(([segments, overrides]) => {
    const snapshot = {
      fetchedAtMs: nowMs,
      range,
      segments,
      overrides,
    };
    touchSnapshot(key, snapshot);
    return snapshot;
  }).finally(() => {
    snapshotPromises.delete(key);
  });
  snapshotPromises.set(key, promise);
  return promise;
}

export function clearDataWebActivitySnapshotCache() {
  snapshotCache.clear();
  snapshotPromises.clear();
}

export function getDataWebActivitySnapshotCacheSizeForTests() {
  return snapshotCache.size;
}

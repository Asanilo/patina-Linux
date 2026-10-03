import { loadHistorySnapshot, type HistorySnapshot } from "./historyReadModel.ts";

import { getUiTextLanguage } from "../../../shared/copy/uiText.ts";

const HISTORY_SNAPSHOT_CACHE_LIMIT = 14;
const HISTORY_SNAPSHOT_CACHE = new Map<string, HistorySnapshot>();

let generation = 0;
export function getHistorySnapshotCacheGeneration(): number { return generation; }

function formatHistorySnapshotCacheKey(date: Date, language: string = getUiTextLanguage()): string {
  const localDate = new Date(date);
  localDate.setHours(0, 0, 0, 0);
  return `${localDate.getFullYear()}-${String(localDate.getMonth() + 1).padStart(2, "0")}-${String(localDate.getDate()).padStart(2, "0")}:${language}`;
}

export function getHistorySnapshotCache(
  date: Date = new Date(),
): HistorySnapshot | null {
  const cacheKey = formatHistorySnapshotCacheKey(date);
  const snapshot = HISTORY_SNAPSHOT_CACHE.get(cacheKey);
  if (!snapshot) return null;

  HISTORY_SNAPSHOT_CACHE.delete(cacheKey);
  HISTORY_SNAPSHOT_CACHE.set(cacheKey, snapshot);
  return snapshot;
}

export function setHistorySnapshotCache(
  snapshot: HistorySnapshot,
  date: Date = new Date(),
): void {
  const cacheKey = formatHistorySnapshotCacheKey(date, snapshot.language);
  HISTORY_SNAPSHOT_CACHE.delete(cacheKey);
  HISTORY_SNAPSHOT_CACHE.set(cacheKey, snapshot);

  while (HISTORY_SNAPSHOT_CACHE.size > HISTORY_SNAPSHOT_CACHE_LIMIT) {
    const oldestKey = HISTORY_SNAPSHOT_CACHE.keys().next().value;
    if (!oldestKey) break;
    HISTORY_SNAPSHOT_CACHE.delete(oldestKey);
  }
}

export function clearHistorySnapshotCache(): void {
  generation++;
  HISTORY_SNAPSHOT_CACHE.clear();
}

export function getHistorySnapshotCacheSizeForTests(): number {
  return HISTORY_SNAPSHOT_CACHE.size;
}

export async function prewarmHistorySnapshotCache(
  date: Date = new Date(),
): Promise<HistorySnapshot> {
  const before = generation;
  const snapshot = await loadHistorySnapshot(date);
  if (before === generation) setHistorySnapshotCache(snapshot, date);
  return snapshot;
}

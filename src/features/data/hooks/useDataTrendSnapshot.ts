import { useEffect, useMemo, useState } from "react";
import {
  resolveDataTrendRange,
  type DataTrendRangeSelection,
  type ResolvedDataTrendRange,
} from "../services/dataTrendRange.ts";

interface TrendSnapshot { fetchedAtMs: number; range: ResolvedDataTrendRange }

interface UseDataTrendSnapshotParams<T extends TrendSnapshot> {
  selection: DataTrendRangeSelection;
  refreshKey: number;
  loadSnapshot: (selection: DataTrendRangeSelection, nowMs?: number) => Promise<T>;
  getCachedSnapshot: (range: ResolvedDataTrendRange) => T | null;
}

export function useDataTrendSnapshot<T extends TrendSnapshot>({
  selection,
  refreshKey,
  loadSnapshot,
  getCachedSnapshot,
}: UseDataTrendSnapshotParams<T>) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const resolvedRange = useMemo(() => resolveDataTrendRange(selection, nowMs), [selection, nowMs]);
  const cached = getCachedSnapshot(resolvedRange);
  const [snapshot, setSnapshot] = useState<T | null>(cached);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(!cached);
  const [hasFetchedOnce, setHasFetchedOnce] = useState(Boolean(cached));

  useEffect(() => {
    let cancelled = false;
    const nextNowMs = Date.now();
    const nextRange = resolveDataTrendRange(selection, nextNowMs);
    const nextCached = getCachedSnapshot(nextRange);
    setError(null);
    if (nextCached) {
      setSnapshot(nextCached);
      setNowMs(nextCached.fetchedAtMs);
      setHasFetchedOnce(true);
      setLoading(false);
    } else {
      setSnapshot(null);
      setLoading(true);
    }

    void loadSnapshot(selection, nextNowMs).then((nextSnapshot) => {
      if (cancelled) return;
      setSnapshot(nextSnapshot);
      setNowMs(nextSnapshot.fetchedAtMs);
      setHasFetchedOnce(true);
    }).catch((failure: unknown) => {
      if (!cancelled) setError(String(failure));
    }).finally(() => {
      if (!cancelled) setLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [getCachedSnapshot, loadSnapshot, refreshKey, selection]);

  return {
    error,
    hasFetchedOnce,
    loading,
    nowMs,
    resolvedRange,
    snapshot,
  };
}

import { useEffect, useMemo, useState } from "react";
import type { DataWebActivitySnapshot } from "../services/dataWebActivitySnapshot.ts";
import {
  resolveDataTrendRange,
  type DataTrendRangeSelection,
} from "../services/dataTrendRange.ts";

interface UseDataWebActivitySnapshotParams {
  enabled: boolean;
  selection: DataTrendRangeSelection;
  refreshKey: number;
  cacheVersion: string;
}

export function useDataWebActivitySnapshot({
  enabled,
  selection,
  refreshKey,
  cacheVersion,
}: UseDataWebActivitySnapshotParams) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const resolvedRange = useMemo(
    () => resolveDataTrendRange(selection, nowMs),
    [nowMs, selection],
  );
  const [snapshot, setSnapshot] = useState<DataWebActivitySnapshot | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!enabled) {
      setSnapshot(null);
      setLoading(false);
      setError(null);
      return undefined;
    }

    let cancelled = false;
    const nextNowMs = Date.now();
    setError(null);
    setLoading(true);

    void import("../services/dataWebActivitySnapshot.ts").then(async (snapshotService) => {
      const nextRange = resolveDataTrendRange(selection, nextNowMs);
      const nextCached = snapshotService.getCachedDataWebActivitySnapshot(nextRange, cacheVersion);
      if (nextCached && !cancelled) {
        setSnapshot(nextCached);
        setNowMs(nextCached.fetchedAtMs);
        setLoading(false);
      }

      const nextSnapshot = await snapshotService.loadDataWebActivitySnapshot(
        selection,
        nextNowMs,
        cacheVersion,
      );
      if (cancelled) return;
      setSnapshot(nextSnapshot);
      setNowMs(nextSnapshot.fetchedAtMs);
    }).catch((loadError: unknown) => {
      if (!cancelled) {
        setError(loadError instanceof Error ? loadError.message : String(loadError));
      }
    }).finally(() => {
      if (!cancelled) setLoading(false);
    });

    return () => {
      cancelled = true;
    };
  }, [cacheVersion, enabled, refreshKey, selection]);

  return {
    error,
    loading,
    nowMs,
    resolvedRange,
    snapshot,
  };
}

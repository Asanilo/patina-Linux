import { startTransition, useEffect, useMemo, useRef, useState } from "react";
import { buildDashboardReadModel, type DashboardReadModel, type DashboardSnapshot } from "../services/dashboardReadModel.ts";
import { getDashboardSnapshotCache, getDashboardSnapshotCacheGeneration, clearDashboardSnapshotCache } from "../services/dashboardSnapshotCache.ts";
import { SnapshotReadController } from "../../../shared/lib/snapshotReadController.ts";
import { getUiTextLanguage } from "../../../shared/copy/uiText.ts";
export interface UseStatsResult {
  dashboard: DashboardReadModel;
  icons: Record<string, string>;
  readError: unknown | null;
}
export function useDashboardStats(
  refreshIntervalSecs: number,
  refreshKey: number,
  loadDashboardSnapshot: (date?: Date) => Promise<DashboardSnapshot>,
  mappingVersion = 0,
  classificationReady = true,
  foregroundRefreshEnabled = true,
): UseStatsResult {
  const [snapshot, setSnapshot] = useState(() => getDashboardSnapshotCache());
  const [readError, setReadError] = useState<unknown | null>(null);
  const controller = useRef<SnapshotReadController<DashboardSnapshot> | null>(null);
  const hasRequestedInitialSnapshot = useRef(false);
  const lastInvalidation = useRef({ refreshKey, mappingVersion });
  const language = getUiTextLanguage();
  useEffect(() => {
    if (!classificationReady) {
      return;
    }
    const owner = new SnapshotReadController(() => loadDashboardSnapshot(new Date()), value => {
      startTransition(() => { setSnapshot(value); setReadError(null); });
    }, error => { setReadError(error ?? new Error("Dashboard unavailable")); console.warn("Failed to load Dashboard snapshot", error); },
    () => `${getDashboardSnapshotCacheGeneration()}:${new Date().toDateString()}:${getUiTextLanguage()}`);
    controller.current = owner;
    if (!hasRequestedInitialSnapshot.current || foregroundRefreshEnabled) {
      hasRequestedInitialSnapshot.current = true;
      owner.refresh();
    }
    const timer = foregroundRefreshEnabled ? window.setInterval(() => owner.refresh(), Math.max(1, refreshIntervalSecs) * 1000) : null;
    return () => {
      if (timer !== null) window.clearInterval(timer);
      owner.dispose();
      if (controller.current === owner) controller.current = null;
    };
  }, [classificationReady, foregroundRefreshEnabled, loadDashboardSnapshot, refreshIntervalSecs, language]);
  useEffect(() => {
    if (lastInvalidation.current.refreshKey !== refreshKey || lastInvalidation.current.mappingVersion !== mappingVersion) {
      clearDashboardSnapshotCache();
      if (foregroundRefreshEnabled)
        controller.current?.refresh(true);
    }
    lastInvalidation.current = { refreshKey, mappingVersion };
  }, [refreshKey, mappingVersion, foregroundRefreshEnabled]);
  const dashboard = useMemo(() => buildDashboardReadModel(classificationReady ? snapshot?.product ?? null : null), [classificationReady, snapshot, mappingVersion, language]);
  return { dashboard, icons: snapshot?.icons ?? {}, readError };
}

import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { HistorySession } from "../../../shared/types/sessions";
import {
  buildDashboardReadModel,
  loadIconSnapshot,
  type DashboardReadModel,
  type DashboardSnapshot,
} from "../services/dashboardReadModel";
import { getDashboardSnapshotCache } from "../services/dashboardSnapshotCache";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking";

export interface UseStatsResult {
  dashboard: DashboardReadModel;
  icons: Record<string, string>;
}

export function useDashboardStats(
  refreshIntervalSecs: number,
  refreshKey: number,
  trackerHealth: TrackerHealthSnapshot,
  loadDashboardSnapshot: (date?: Date) => Promise<DashboardSnapshot>,
  mappingVersion: number = 0,
  classificationReady: boolean = true,
  foregroundRefreshEnabled: boolean = true,
): UseStatsResult {
  const initialSnapshot = getDashboardSnapshotCache();
  const hasRequestedInitialSnapshotRef = useRef(false);
  const [rawSessions, setRawSessions] = useState<HistorySession[]>(
    () => initialSnapshot?.sessions ?? [],
  );
  const [rawYesterdaySessions, setRawYesterdaySessions] = useState<HistorySession[]>(
    () => initialSnapshot?.yesterdaySessions ?? [],
  );
  const [icons, setIcons] = useState<Record<string, string>>(
    () => initialSnapshot?.icons ?? {},
  );
  const [nowMs, setNowMs] = useState(() => initialSnapshot?.fetchedAtMs ?? Date.now());

  const loadSnapshot = useCallback(async () => {
    try {
      const snapshot = await loadDashboardSnapshot(new Date());

      startTransition(() => {
        setRawSessions(snapshot.sessions);
        setRawYesterdaySessions(snapshot.yesterdaySessions ?? []);
        setIcons(snapshot.icons);
        setNowMs(snapshot.fetchedAtMs);
      });
    } catch (err) {
      console.error("Failed to load stats:", err);
    }
  }, [loadDashboardSnapshot]);

  useEffect(() => {
    if (!classificationReady || hasRequestedInitialSnapshotRef.current) return;
    hasRequestedInitialSnapshotRef.current = true;

    void loadSnapshot();
  }, [classificationReady, loadSnapshot]);

  useEffect(() => {
    if (refreshKey === 0 || !classificationReady || !foregroundRefreshEnabled) return;
    void loadSnapshot();
  }, [classificationReady, foregroundRefreshEnabled, refreshKey, loadSnapshot]);

  useEffect(() => {
    const hasLiveSession = rawSessions.some((session) => session.endTime === null);
    if (!classificationReady || !foregroundRefreshEnabled || !hasLiveSession || trackerHealth.status !== "healthy") {
      return;
    }

    const hasMissingIcons = rawSessions.some((session) => !icons[session.exeName]);

    const timer = window.setInterval(() => {
      setNowMs(Date.now());

      if (hasMissingIcons) {
        void loadIconSnapshot()
          .then((snapshot) => {
            startTransition(() => {
              setIcons(snapshot.icons);
            });
          })
          .catch((error) => {
            console.warn("Failed to refresh icon cache:", error);
          });
      }
    }, refreshIntervalSecs * 1000);

    return () => {
      window.clearInterval(timer);
    };
  }, [classificationReady, icons, rawSessions, foregroundRefreshEnabled, refreshIntervalSecs, trackerHealth.status]);

  const dashboard = useMemo(
    () => buildDashboardReadModel(
      classificationReady ? rawSessions : [],
      trackerHealth,
      nowMs,
      classificationReady ? rawYesterdaySessions : [],
    ),
    [classificationReady, mappingVersion, nowMs, rawSessions, rawYesterdaySessions, trackerHealth],
  );

  return {
    dashboard,
    icons,
  };
}

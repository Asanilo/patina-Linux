import { startTransition, useCallback, useEffect, useRef, useState } from "react";
import { SnapshotReadController } from "../../../shared/lib/snapshotReadController.ts";
import { getUiTextLanguage } from "../../../shared/copy/uiText.ts";
import { formatLocalDateKey } from "../../../shared/lib/localDate.ts";
import {
  loadDestinationDetailDay,
  type DestinationDetailDayViewModel,
} from "../services/destinationDetailReadModel.ts";
import {
  encodeDestinationDetailDayRequestKey,
  isDestinationDetailDateKeyAvailable,
  resolveDestinationDetailInitialDateKey,
} from "../services/destinationDetailState.ts";
import type { DestinationDetailTarget } from "../types.ts";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking.ts";

type DetailLoadStatus = "loading" | "refreshing" | "ready" | "error";

interface Params {
  target: DestinationDetailTarget;
  initialDateKey: string;
  refreshKey: number;
  mappingVersion: number;
  mergeThresholdSecs: number;
  trackerHealth: TrackerHealthSnapshot;
}

interface DetailDayState {
  requestKey: string;
  viewModel: DestinationDetailDayViewModel | null;
  status: DetailLoadStatus;
  error: Error | null;
}

export function useDestinationDetail({
  target,
  initialDateKey,
  refreshKey,
  mappingVersion,
  mergeThresholdSecs,
  trackerHealth,
}: Params) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const [focusedDateKey, setFocusedDateKeyState] = useState(() => (
    resolveDestinationDetailInitialDateKey(initialDateKey, nowMs)
  ));
  const healthRef = useRef(trackerHealth);
  healthRef.current = trackerHealth;
  const language = getUiTextLanguage();
  const [retryRevision, setRetryRevision] = useState(0);
  const [dayState, setDayState] = useState<DetailDayState>({
    requestKey: "",
    viewModel: null,
    status: "loading",
    error: null,
  });
  const cacheVersion = `${mappingVersion}:${refreshKey}`;
  const cacheScope = useRef(cacheVersion);
  cacheScope.current = cacheVersion;
  const controller = useRef<SnapshotReadController<DestinationDetailDayViewModel> | null>(null);
  const previousVersion = useRef(cacheVersion);
  useEffect(() => {
    if (previousVersion.current !== cacheVersion) controller.current?.refresh(true);
    previousVersion.current = cacheVersion;
  }, [cacheVersion]);
  useEffect(() => {
    const requestNowMs = Date.now();
    setNowMs(requestNowMs);
    const requestKey = encodeDestinationDetailDayRequestKey(
      target,
      focusedDateKey,
      cacheVersion,
    );
    let failed = false;
    setDayState((current) => {
      const sameTarget = current.requestKey.startsWith(`${target.mode}:${target.key}:`)
        && current.viewModel?.dateKey === focusedDateKey;
      return {
        requestKey,
        viewModel: sameTarget ? current.viewModel : null,
        status: sameTarget && current.viewModel ? "refreshing" : "loading",
        error: null,
      };
    });
    const owner = new SnapshotReadController(
      () => loadDestinationDetailDay(
        target, focusedDateKey, Date.now(), mergeThresholdSecs, undefined,
        healthRef.current.status, healthRef.current.lastHeartbeatMs,
      ),
      viewModel => {
        failed = false;
        setNowMs(Date.now());
        startTransition(() => setDayState({requestKey, viewModel, status: "ready", error: null}));
      },
      error => {
        failed = true;
        setDayState(current => ({...current, status: "error",
          error: error instanceof Error ? error : new Error(String(error))}));
      },
      () => `${cacheScope.current}:${getUiTextLanguage()}`,
    );
    controller.current = owner;
    owner.refresh();
    const timer = window.setInterval(() => {
      if (failed || focusedDateKey === formatLocalDateKey(new Date())) owner.refresh();
    }, 2000);
    return () => {
      window.clearInterval(timer);
      owner.dispose();
      if (controller.current === owner) controller.current = null;
    };
  }, [
    focusedDateKey,
    mergeThresholdSecs,
    retryRevision,
    target,
    language,
  ]);

  const setFocusedDateKey = useCallback((dateKey: string) => {
    if (isDestinationDetailDateKeyAvailable(dateKey, nowMs)) {
      setFocusedDateKeyState(dateKey);
    }
  }, [nowMs]);

  return {
    nowMs,
    todayDateKey: formatLocalDateKey(new Date(nowMs)),
    focusedDateKey,
    setFocusedDateKey,
    day: dayState,
    retryDay: () => setRetryRevision((current) => current + 1),
  };
}

import type { HistorySession } from "../../../shared/types/sessions.ts";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking.ts";
import type {
  WebActivitySegment,
  WebDomainOverride,
} from "../../../shared/types/webActivity.ts";
import type { ExactHistoryRead } from "../../../platform/persistence/historyRepository.ts";
import { getUiTextLanguage, type UiLanguage } from "../../../shared/copy/uiText.ts";
import {
  getWebActivitySegmentsInRange,
  loadWebDomainOverrides,
} from "../../../platform/persistence/webActivityRepository.ts";
import {
  buildHourlyActivity,
  buildHourlyCategoryActivity,
  type HourlyActivityPoint,
  type HourlyCategoryActivity,
} from "../../../shared/lib/hourlyActivityCompiler.ts";
import {
  buildAppSummary,
  buildNormalizedAppStats,
  buildTimelineSessions,
  getDayRange,
  type NormalizedAppSummaryItem,
  type TimelineSession,
} from "../../../shared/lib/sessionReadCompiler.ts";
import {
  buildReadModelDiagnostics,
  compileForRange,
  materializeLiveSessions,
  resolveLiveCutoffMs,
  type ReadModelDiagnostics,
} from "../../../shared/lib/readModelCore.ts";

export interface HistorySnapshot {
  fetchedAtMs: number;
  language: UiLanguage;
  trackerHealth: TrackerHealthSnapshot;
  liveCutoffMs: number;
  daySessions: HistorySession[];
  dayWebSegments: WebActivitySegment[];
  webDomainOverrides: Record<string, WebDomainOverride>;
}

export interface HistoryReadModel {
  compiledSessions: ReturnType<typeof compileForRange>;
  timelineSessions: TimelineSession[];
  appSummary: NormalizedAppSummaryItem[];
  hourlyActivity: HourlyActivityPoint[];
  hourlyCategoryActivity: HourlyCategoryActivity;
  diagnostics: ReadModelDiagnostics;
}

interface HistorySnapshotDeps {
  getExactHistory: (from: number, to: number, language: UiLanguage) => Promise<ExactHistoryRead>;
  getWebActivitySegmentsInRange: typeof getWebActivitySegmentsInRange;
  loadWebDomainOverrides: typeof loadWebDomainOverrides;
}

const DEFAULT_HISTORY_SNAPSHOT_DEPS: HistorySnapshotDeps = {
  getExactHistory: async (from, to, language) => {
    const { getExactHistorySnapshot } = await import("../../../platform/persistence/historyRepository.ts");
    return getExactHistorySnapshot(from, to, undefined, language);
  },
  getWebActivitySegmentsInRange,
  loadWebDomainOverrides,
};

let warnedWebHistoryFallback = false;

async function loadOptionalWebSnapshotPart(
  deps: HistorySnapshotDeps,
  selectedDayRange: { startMs: number; endMs: number },
): Promise<Pick<HistorySnapshot, "dayWebSegments" | "webDomainOverrides">> {
  try {
    const [dayWebSegments, webDomainOverrides] = await Promise.all([
      deps.getWebActivitySegmentsInRange(selectedDayRange.startMs, selectedDayRange.endMs),
      deps.loadWebDomainOverrides(),
    ]);

    return {
      dayWebSegments,
      webDomainOverrides,
    };
  } catch (error) {
    if (!warnedWebHistoryFallback) {
      warnedWebHistoryFallback = true;
      console.warn("History web activity data is unavailable; continuing with app history only.", error);
    }
    return {
      dayWebSegments: [],
      webDomainOverrides: {},
    };
  }
}

function filterTimelineSessionsForDisplay(
  sessions: TimelineSession[],
  minSessionSecs: number,
) {
  const minDurationMs = Math.max(0, minSessionSecs) * 1000;
  if (minDurationMs <= 0) {
    return sessions;
  }

  return sessions.filter((session) => (
    (session.duration ?? 0) >= minDurationMs
  ));
}

export async function loadHistorySnapshot(
  date: Date,
  deps: HistorySnapshotDeps = DEFAULT_HISTORY_SNAPSHOT_DEPS,
): Promise<HistorySnapshot> {
  const selectedDayRange = getDayRange(date, Number.MAX_SAFE_INTEGER);
  const language = getUiTextLanguage();
  const [read, webSnapshotPart] = await Promise.all([
    deps.getExactHistory(selectedDayRange.startMs, selectedDayRange.endMs, language),
    loadOptionalWebSnapshotPart(deps, selectedDayRange),
  ]);
  if (language !== getUiTextLanguage()) throw new Error("History language changed during read");
  return {
    fetchedAtMs: read.sampledAtMs,
    language,
    trackerHealth: { status: read.trackingHealth.status === "healthy" ? "healthy" : "stale",
      lastHeartbeatMs: read.trackingHealth.lastHeartbeatMs, checkedAtMs: read.sampledAtMs, staleAfterMs: read.trackingHealth.staleAfterMs },
    liveCutoffMs: read.trackingHealth.liveCutoffMs,
    daySessions: read.sessions,
    dayWebSegments: webSnapshotPart.dayWebSegments,
    webDomainOverrides: webSnapshotPart.webDomainOverrides,
  };
}

export function buildHistoryReadModel(params: {
  daySessions: HistorySession[];
  trackerHealth: TrackerHealthSnapshot;
  selectedDate: Date;
  nowMs: number;
  minSessionSecs: number;
  mergeThresholdSecs: number;
}): HistoryReadModel {
  const {
    daySessions,
    trackerHealth,
    selectedDate,
    nowMs,
    minSessionSecs,
    mergeThresholdSecs,
  } = params;
  const selectedDayRange = getDayRange(selectedDate, nowMs);
  const liveDaySessions = materializeLiveSessions(daySessions, trackerHealth, nowMs);
  const compiledSessions = compileForRange(liveDaySessions, selectedDayRange, 0);
  const mergedTimelineSessions = buildTimelineSessions(compiledSessions, mergeThresholdSecs);
  const timelineSessions = filterTimelineSessionsForDisplay(
    mergedTimelineSessions,
    minSessionSecs,
  ).slice().reverse();
  const appSummary = buildAppSummary(buildNormalizedAppStats(compiledSessions));
  const hourlyActivity = buildHourlyActivity(compiledSessions);
  const hourlyCategoryActivity = buildHourlyCategoryActivity(compiledSessions);
  const diagnostics = buildReadModelDiagnostics(
    compiledSessions,
    trackerHealth,
    resolveLiveCutoffMs(trackerHealth, nowMs),
  );

  // Keep read-model shaping in memory only for now. The hot paths get lighter
  // without introducing persistent summary tables or premature caching.
  return {
    compiledSessions,
    timelineSessions,
    appSummary,
    hourlyActivity,
    hourlyCategoryActivity,
    diagnostics,
  };
}

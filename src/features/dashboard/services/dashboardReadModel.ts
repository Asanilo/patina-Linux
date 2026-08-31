import type { AppStat } from "../../../shared/types/app.ts";
import type { HistorySession } from "../../../shared/types/sessions.ts";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking.ts";
import {
  getIconMap,
  getSessionSummariesInRange,
} from "../../../platform/persistence/sessionReadRepository.ts";
import {
  buildCategoryDistribution,
  buildTopApplications,
  getTotalTrackedTime,
  type CategoryDistItem,
  type TopApplicationItem,
} from "./dashboardFormatting.ts";
import {
  buildHourlyActivity,
  buildHourlyCategoryActivity,
  type HourlyActivityPoint,
  type HourlyCategoryActivity,
} from "../../../shared/lib/hourlyActivityCompiler.ts";
import {
  buildNormalizedAppStats,
  getDayRange,
  type CompiledSession,
} from "../../../shared/lib/sessionReadCompiler.ts";
import {
  buildReadModelDiagnostics,
  compileForRange,
  materializeLiveSessions,
  resolveLiveCutoffMs,
  type ReadModelDiagnostics,
} from "../../../shared/lib/readModelCore.ts";

export interface DashboardSnapshot {
  fetchedAtMs: number;
  icons: Record<string, string>;
  sessions: DashboardActivityRecord[];
  yesterdaySessions?: DashboardActivityRecord[];
}

export interface DashboardActivityRecord {
  appName: string;
  exeName: string;
  startTime: number;
  endTime: number | null;
  isLive?: boolean;
}

export interface IconSnapshot {
  fetchedAtMs: number;
  icons: Record<string, string>;
}

export interface DashboardReadModel {
  compiledSessions: CompiledSession[];
  stats: AppStat[];
  totalTrackedTime: number;
  yesterdayTrackedTime: number;
  dayDeltaTrackedTime: number;
  topApplications: TopApplicationItem[];
  hourlyActivity: HourlyActivityPoint[];
  hourlyCategoryActivity: HourlyCategoryActivity;
  categoryDist: CategoryDistItem[];
  diagnostics: ReadModelDiagnostics;
}

export async function loadDashboardSnapshot(
  date: Date = new Date(),
): Promise<DashboardSnapshot> {
  const fetchedAtMs = Date.now();
  const dayRange = getDayRange(date, fetchedAtMs);
  const yesterday = new Date(date);
  yesterday.setDate(yesterday.getDate() - 1);
  const yesterdayRange = getDayRange(yesterday, fetchedAtMs);
  const [sessions, yesterdaySessions, icons] = await Promise.all([
    getSessionSummariesInRange(dayRange.startMs, dayRange.endMs),
    getSessionSummariesInRange(yesterdayRange.startMs, yesterdayRange.endMs),
    getIconMap(),
  ]);

  return {
    fetchedAtMs,
    icons,
    sessions,
    yesterdaySessions,
  };
}

function toDashboardHistorySessions(
  records: DashboardActivityRecord[],
): HistorySession[] {
  return records.map((record, index) => {
    const endTime = Math.max(record.startTime, record.endTime ?? record.startTime);
    const isLive = record.isLive ?? record.endTime === null;

    return {
      id: -(index + 1),
      appName: record.appName,
      exeName: record.exeName,
      windowTitle: "",
      startTime: record.startTime,
      endTime: isLive ? null : endTime,
      duration: endTime - record.startTime,
      continuityGroupStartTime: record.startTime,
      titleSampleDetails: [],
    };
  });
}

export async function loadIconSnapshot(): Promise<IconSnapshot> {
  const icons = await getIconMap();

  return {
    fetchedAtMs: Date.now(),
    icons,
  };
}

export function buildDashboardReadModel(
  sessions: DashboardActivityRecord[],
  trackerHealth: TrackerHealthSnapshot,
  nowMs: number,
  yesterdaySessions: DashboardActivityRecord[] = [],
): DashboardReadModel {
  const dayRange = getDayRange(new Date(nowMs), nowMs);
  const yesterday = new Date(nowMs);
  yesterday.setDate(yesterday.getDate() - 1);
  const yesterdayRange = getDayRange(yesterday, nowMs);
  const liveSessions = materializeLiveSessions(
    toDashboardHistorySessions(sessions),
    trackerHealth,
    nowMs,
  );
  const compiledSessions = compileForRange(liveSessions, dayRange, 0);
  const compiledYesterdaySessions = compileForRange(
    toDashboardHistorySessions(yesterdaySessions),
    yesterdayRange,
    0,
  );
  const stats = buildNormalizedAppStats(compiledSessions);
  const yesterdayStats = buildNormalizedAppStats(compiledYesterdaySessions);
  const totalTrackedTime = getTotalTrackedTime(stats);
  const yesterdayTrackedTime = getTotalTrackedTime(yesterdayStats);
  const diagnostics = buildReadModelDiagnostics(
    compiledSessions,
    trackerHealth,
    resolveLiveCutoffMs(trackerHealth, nowMs),
  );

  return {
    compiledSessions,
    stats,
    totalTrackedTime,
    yesterdayTrackedTime,
    dayDeltaTrackedTime: totalTrackedTime - yesterdayTrackedTime,
    topApplications: buildTopApplications(stats),
    hourlyActivity: buildHourlyActivity(compiledSessions),
    hourlyCategoryActivity: buildHourlyCategoryActivity(compiledSessions),
    categoryDist: buildCategoryDistribution(stats),
    diagnostics,
  };
}

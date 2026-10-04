// Historical replay oracle only. Production Dashboard consumes backend product snapshots.
import type { AppStat } from "../../src/shared/types/app.ts";
import type { HistorySession } from "../../src/shared/types/sessions.ts";
import type { TrackerHealthSnapshot } from "../../src/shared/types/tracking.ts";
import {
  buildCategoryDistribution,
  buildTopApplications,
  getTotalTrackedTime,
  type CategoryDistItem,
  type TopApplicationItem,
} from "./legacyDashboardFormatting.ts";
import {
  buildHourlyActivity,
  buildHourlyCategoryActivity,
  type HourlyActivityPoint,
  type HourlyCategoryActivity,
} from "./legacyHourlyActivityCompiler.ts";
import {
  buildNormalizedAppStats,
  getDayRange,
  type CompiledSession,
} from "../../src/shared/lib/sessionReadCompiler.ts";
import {
  buildReadModelDiagnostics,
  compileForRange,
  materializeLiveSessions,
  resolveLiveCutoffMs,
  type ReadModelDiagnostics,
} from "../../src/shared/lib/readModelCore.ts";

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

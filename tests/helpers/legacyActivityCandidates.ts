// Legacy candidate replay oracle; production Dashboard no longer reads aggregate SQL.
import { AppClassification } from "../../src/shared/classification/appClassification.ts";
import { resolveNativeSessionPrecedence, type ActivityResolutionScope, type TimeRecordOrigin } from "./legacyNativeSessionPrecedence.ts";
import type { AggregateSessionRecord } from "../../src/shared/types/sessions.ts";
export interface RawAggregateSessionCandidateRow {
  record_id?: number;
  origin?: TimeRecordOrigin;
  app_name: string;
  exe_name: string;
  window_title: string;
  start_time: number;
  effective_end_time: number;
  capacity_end_time?: number;
  is_live?: number | boolean;
}

export function mapRawAggregateSessionCandidates(
  rows: RawAggregateSessionCandidateRow[],
  scope?: ActivityResolutionScope,
): AggregateSessionRecord[] {
  const resolved = resolveNativeSessionPrecedence(
    rows.map((row, index) => ({
      key: `${row.origin ?? "native"}:${row.record_id ?? index}`,
      origin: row.origin ?? "native",
      startTime: row.start_time,
      endTime: Math.max(row.start_time, row.effective_end_time),
      capacityEndTime: row.capacity_end_time,
      value: row,
    })),
    scope,
  );
  return resolved
    .map((range) => ({
      ...range.value!,
      start_time: range.startTime,
      effective_end_time: range.endTime,
    }))
    .filter((row) => AppClassification.shouldTrackProcess(row.exe_name, {
      appName: row.app_name,
      windowTitle: row.window_title,
    }))
    .map((row) => ({
      appName: row.app_name,
      exeName: row.exe_name,
      startTime: row.start_time,
      endTime: Math.max(row.start_time, row.effective_end_time),
      ...(row.is_live ? { isLive: true } : {}),
    }));
}


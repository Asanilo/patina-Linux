import { getDB } from "./sqlite.ts";
import { AppClassification } from "../../shared/classification/appClassification.ts";
import type { HistorySession, TitleSampleDetail } from "../../shared/types/sessions.ts";
import {
  resolveNativeSessionPrecedence,
  type ActivityResolutionScope,
  type TimeRecordOrigin,
} from "./nativeSessionPrecedence.ts";

interface RawHistorySessionRow {
  id: number;
  origin: Exclude<TimeRecordOrigin, "import_bucket">;
  app_name: string;
  exe_name: string;
  window_title: string;
  start_time: number;
  end_time: number | null;
  duration: number | null;
  continuity_group_start_time: number | null;
}

interface RawTitleSampleRow {
  session_id: number;
  title: string;
  start_time: number;
  end_time: number | null;
}

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

export interface AggregateSessionRecord {
  appName: string;
  exeName: string;
  startTime: number;
  endTime: number;
  isLive?: boolean;
}

function mapRawTitleSample(row: RawTitleSampleRow): TitleSampleDetail {
  return {
    title: row.title,
    startTime: row.start_time,
    endTime: row.end_time,
  };
}

function mapRawHistorySession(
  row: RawHistorySessionRow,
  titleSampleDetails: TitleSampleDetail[] = [],
): HistorySession {
  return {
    id: row.id,
    appName: row.app_name,
    exeName: row.exe_name,
    windowTitle: row.window_title,
    startTime: row.start_time,
    endTime: row.end_time,
    duration: row.duration,
    continuityGroupStartTime: row.continuity_group_start_time,
    titleSampleDetails,
  };
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

export async function getIconMap(): Promise<Record<string, string>> {
  const db = await getDB();
  const results = await db.select<{ exe_name: string; icon_base64: string }[]>(
    "SELECT exe_name, icon_base64 FROM icon_cache",
  );
  const map: Record<string, string> = {};

  for (const row of results) {
    const rawExe = (row.exe_name ?? "").trim();
    if (!rawExe) continue;

    const normalizedExe = AppClassification.resolveCanonicalExecutable(rawExe);
    const lowerExe = rawExe.toLowerCase();

    map[rawExe] = row.icon_base64;
    map[lowerExe] = row.icon_base64;
    map[normalizedExe] = row.icon_base64;
  }

  return map;
}

export async function getSessionsInRange(startMs: number, endMs: number): Promise<HistorySession[]> {
  const db = await getDB();
  const now = Date.now();
  const rows = await db.select<RawHistorySessionRow[]>(
    `SELECT id, 'native' AS origin, app_name, exe_name, COALESCE(window_title, '') AS window_title,
            start_time, end_time, COALESCE(duration, MAX(0, ? - start_time)) AS duration,
            continuity_group_start_time
     FROM sessions
     WHERE start_time < ? AND COALESCE(end_time, ?) > ?
     UNION ALL
     SELECT id, 'import_exact' AS origin, app_name, exe_name, window_title,
            start_time, end_time, duration, start_time AS continuity_group_start_time
     FROM import_exact_sessions
     WHERE start_time < ? AND end_time > ?
     ORDER BY start_time ASC, origin ASC, id ASC`,
    [now, endMs, now, startMs, endMs, startMs],
  );

  if (rows.length === 0) {
    return [];
  }

  const samplesBySessionId = new Map<number, TitleSampleDetail[]>();
  const sessionIds = rows.filter((row) => row.origin === "native").map((row) => row.id);
  const batchSize = 900;
  for (let index = 0; index < sessionIds.length; index += batchSize) {
    const batchIds = sessionIds.slice(index, index + batchSize);
    const placeholders = batchIds.map(() => "?").join(", ");
    const sampleRows = await db.select<RawTitleSampleRow[]>(
      `SELECT session_id, title, start_time, end_time
       FROM session_title_samples
       WHERE session_id IN (${placeholders})
         AND start_time < ?
         AND COALESCE(end_time, ?) > ?
       ORDER BY session_id ASC, start_time ASC, id ASC`,
      [...batchIds, endMs, now, startMs],
    );

    for (const sampleRow of sampleRows) {
      const samples = samplesBySessionId.get(sampleRow.session_id) ?? [];
      samples.push(mapRawTitleSample(sampleRow));
      samplesBySessionId.set(sampleRow.session_id, samples);
    }
  }

  const resolved = resolveNativeSessionPrecedence(rows.map((row) => ({
    key: `${row.origin}:${row.id}`,
    origin: row.origin,
    startTime: row.start_time,
    endTime: row.end_time ?? now,
    value: row,
  })));

  let importedSequence = 0;
  return resolved.map((range) => {
    const row = range.value!;
    const adjusted: RawHistorySessionRow = {
      ...row,
      id: row.origin === "native" ? row.id : -(++importedSequence),
      start_time: range.startTime,
      end_time: range.endTime,
      duration: range.endTime - range.startTime,
      continuity_group_start_time: range.startTime,
    };
    const samples = row.origin === "native"
      ? samplesBySessionId.get(row.id) ?? []
      : row.window_title.trim()
        ? [{ title: row.window_title, startTime: range.startTime, endTime: range.endTime }]
        : [];
    return mapRawHistorySession(adjusted, samples);
  });
}

export async function getSessionSummariesInRange(startMs: number, endMs: number): Promise<AggregateSessionRecord[]> {
  const db = await getDB();
  const now = Date.now();
  const rows = await db.select<RawAggregateSessionCandidateRow[]>(
    `SELECT id AS record_id, 'native' AS origin, app_name, exe_name,
            COALESCE(window_title, '') AS window_title, start_time,
            COALESCE(end_time, ?) AS effective_end_time,
            COALESCE(end_time, ?) AS capacity_end_time,
            end_time IS NULL AS is_live
     FROM sessions
     WHERE start_time < ? AND COALESCE(end_time, ?) > ?
     UNION ALL
     SELECT id, 'import_exact', app_name, exe_name, window_title, start_time,
            end_time, end_time, 0
     FROM import_exact_sessions
     WHERE start_time < ? AND end_time > ?
     UNION ALL
     SELECT id, 'import_bucket', app_name, exe_name, '' AS window_title,
            bucket_start_time AS start_time,
            bucket_start_time + duration AS effective_end_time,
            bucket_start_time + 3600000 AS capacity_end_time,
            0
     FROM import_time_buckets
     WHERE bucket_start_time < ? AND bucket_start_time + 3600000 > ?
     ORDER BY start_time ASC, origin ASC, record_id ASC`,
    [now, now, endMs, now, startMs, endMs, startMs, endMs, startMs],
  );
  return mapRawAggregateSessionCandidates(rows, { startTime: startMs, endTime: endMs });
}

export async function getEarliestSessionStartTime(): Promise<number | null> {
  const db = await getDB();
  const rows = await db.select<{ earliest_start_time: number | null }[]>(
    `SELECT MIN(start_time) AS earliest_start_time
     FROM (
       SELECT start_time FROM sessions
       UNION ALL
       SELECT start_time FROM import_exact_sessions
       UNION ALL
       SELECT bucket_start_time AS start_time FROM import_time_buckets
     )`,
  );
  return rows[0]?.earliest_start_time ?? null;
}

export async function getHistoryByDate(date: Date): Promise<HistorySession[]> {
  const start = new Date(date);
  start.setHours(0, 0, 0, 0);
  const end = new Date(date);
  end.setHours(24, 0, 0, 0);
  return getSessionsInRange(start.getTime(), end.getTime());
}

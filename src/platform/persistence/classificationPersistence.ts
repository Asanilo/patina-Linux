import { executeWrite, executeWriteBatch, getDB } from "./sqlite.ts";
import { resolveNativeSessionPrecedence, type TimeRecordOrigin } from "./nativeSessionPrecedence.ts";

export interface SettingKeyValueRow {
  key: string;
  value: string;
}

export interface SettingKeyRow {
  key: string;
}

interface RawSessionExeNameRow {
  exe_name: string;
}

interface RawObservedSessionStatRow {
  id: number;
  origin: TimeRecordOrigin;
  exe_name: string;
  app_name: string;
  start_time: number;
  end_time: number;
  capacity_end_time: number;
}

export interface SessionExeNameRow {
  exeName: string;
}

export interface ObservedSessionStatRow {
  exeName: string;
  appName: string;
  totalDuration: number;
  lastSeenMs: number;
}

export async function upsertSettingValue(key: string, value: string): Promise<void> {
  await executeWrite(
    "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    [key, value],
  );
}

export async function deleteSettingValue(key: string): Promise<void> {
  await executeWrite("DELETE FROM settings WHERE key = ?", [key]);
}

export async function loadSettingValue(key: string): Promise<string | null> {
  const db = await getDB();
  const rows = await db.select<{ value: string }[]>(
    "SELECT value FROM settings WHERE key = ? LIMIT 1",
    [key],
  );
  return rows[0]?.value ?? null;
}

export async function loadSettingRowsByKeyPrefix(keyPrefix: string): Promise<SettingKeyValueRow[]> {
  const db = await getDB();
  return db.select<SettingKeyValueRow[]>(
    "SELECT key, value FROM settings WHERE key LIKE ?",
    [`${keyPrefix}%`],
  );
}

export async function loadSettingKeysByKeyPrefix(keyPrefix: string): Promise<SettingKeyRow[]> {
  const db = await getDB();
  return db.select<SettingKeyRow[]>(
    "SELECT key FROM settings WHERE key LIKE ?",
    [`${keyPrefix}%`],
  );
}

export async function loadDistinctSessionExeNames(): Promise<SessionExeNameRow[]> {
  const db = await getDB();
  const rows = await db.select<RawSessionExeNameRow[]>(
    `SELECT DISTINCT exe_name FROM (
       SELECT exe_name FROM sessions
       UNION ALL SELECT exe_name FROM import_exact_sessions
       UNION ALL SELECT exe_name FROM import_time_buckets
     )`,
  );
  return rows.map((row) => ({
    exeName: row.exe_name,
  }));
}

export async function loadObservedSessionStats(
  sinceMs: number,
  nowMs: number,
): Promise<ObservedSessionStatRow[]> {
  const db = await getDB();
  const rows = await db.select<RawObservedSessionStatRow[]>(
    `SELECT id, 'native' AS origin, exe_name, COALESCE(app_name, '') AS app_name,
            start_time, COALESCE(end_time, ?) AS end_time,
            COALESCE(end_time, ?) AS capacity_end_time
     FROM sessions WHERE start_time < ? AND COALESCE(end_time, ?) > ?
     UNION ALL
     SELECT id, 'import_exact', exe_name, app_name, start_time, end_time, end_time
     FROM import_exact_sessions WHERE start_time < ? AND end_time > ?
     UNION ALL
     SELECT id, 'import_bucket', exe_name, app_name, bucket_start_time,
            bucket_start_time + duration, bucket_start_time + 3600000
     FROM import_time_buckets
     WHERE bucket_start_time < ? AND bucket_start_time + 3600000 > ?`,
    [nowMs, nowMs, nowMs, nowMs, sinceMs, nowMs, sinceMs, nowMs, sinceMs],
  );
  const resolved = resolveNativeSessionPrecedence(
    rows.map((row) => ({
      key: `${row.origin}:${row.id}`,
      origin: row.origin,
      startTime: row.start_time,
      endTime: row.end_time,
      capacityEndTime: row.capacity_end_time,
      value: row,
    })),
    { startTime: sinceMs, endTime: nowMs },
  );
  const byExe = new Map<string, ObservedSessionStatRow>();
  for (const range of resolved) {
    const row = range.value!;
    const current = byExe.get(row.exe_name);
    const totalDuration = (current?.totalDuration ?? 0) + range.endTime - range.startTime;
    const lastSeenMs = Math.max(current?.lastSeenMs ?? 0, range.startTime);
    byExe.set(row.exe_name, {
      exeName: row.exe_name,
      appName: lastSeenMs === range.startTime ? row.app_name : current?.appName ?? row.app_name,
      totalDuration,
      lastSeenMs,
    });
  }
  return Array.from(byExe.values());
}

function buildInClausePlaceholders(values: readonly string[]): string {
  return values.map(() => "?").join(", ");
}

export async function deleteSessionsByExeNames(exeNames: string[]): Promise<void> {
  if (exeNames.length === 0) {
    return;
  }
  const placeholders = buildInClausePlaceholders(exeNames);
  await executeWriteBatch([
    { query: `DELETE FROM sessions WHERE exe_name IN (${placeholders})`, values: exeNames },
    { query: `DELETE FROM import_exact_sessions WHERE exe_name IN (${placeholders})`, values: exeNames },
    { query: `DELETE FROM import_time_buckets WHERE exe_name IN (${placeholders})`, values: exeNames },
    { query: "UPDATE import_batches SET exact_session_count = (SELECT COUNT(*) FROM import_exact_sessions WHERE batch_id = import_batches.id), hour_bucket_count = (SELECT COUNT(*) FROM import_time_buckets WHERE batch_id = import_batches.id)" },
    { query: "DELETE FROM import_batches WHERE exact_session_count = 0 AND hour_bucket_count = 0" },
  ]);
}

export async function deleteSessionsByExeNamesBetween(
  exeNames: string[],
  startTime: number,
  endTime: number,
): Promise<void> {
  if (exeNames.length === 0) {
    return;
  }
  const placeholders = buildInClausePlaceholders(exeNames);
  await executeWriteBatch([
    {
      query: `DELETE FROM sessions WHERE exe_name IN (${placeholders}) AND start_time >= ? AND start_time < ?`,
      values: [...exeNames, startTime, endTime],
    },
    {
      query: `DELETE FROM import_exact_sessions WHERE exe_name IN (${placeholders}) AND start_time >= ? AND start_time < ?`,
      values: [...exeNames, startTime, endTime],
    },
    {
      query: `DELETE FROM import_time_buckets WHERE exe_name IN (${placeholders}) AND bucket_start_time >= ? AND bucket_start_time < ?`,
      values: [...exeNames, startTime, endTime],
    },
    { query: "UPDATE import_batches SET exact_session_count = (SELECT COUNT(*) FROM import_exact_sessions WHERE batch_id = import_batches.id), hour_bucket_count = (SELECT COUNT(*) FROM import_time_buckets WHERE batch_id = import_batches.id)" },
    { query: "DELETE FROM import_batches WHERE exact_session_count = 0 AND hour_bucket_count = 0" },
  ]);
}

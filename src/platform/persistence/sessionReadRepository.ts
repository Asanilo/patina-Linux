import { getDB } from "./sqlite.ts";
import { AppClassification } from "../../shared/classification/appClassification.ts";
import type { HistorySession } from "../../shared/types/sessions.ts";

export interface AggregateSessionRecord {
  appName: string;
  exeName: string;
  startTime: number;
  endTime: number;
  isLive?: boolean;
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
  const { getExactHistorySnapshot } = await import("./historyRepository.ts");
  return (await getExactHistorySnapshot(startMs, endMs)).sessions;
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

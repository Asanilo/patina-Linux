import { executeWrite, executeWriteBatch, getDB } from "./sqlite.ts";

export interface SettingRow {
  key: string;
  value: string;
}

export async function upsertSettingValue(key: string, value: string): Promise<void> {
  await executeWrite(
    "INSERT INTO settings (key, value) VALUES (?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
    [key, value],
  );
}

export async function loadSettingTimestamp(key: string): Promise<number | null> {
  const db = await getDB();
  const rows = await db.select<{ value: string }[]>(
    "SELECT value FROM settings WHERE key = ? LIMIT 1",
    [key],
  );

  if (rows.length === 0) {
    return null;
  }

  const parsed = Number(rows[0].value);
  return Number.isFinite(parsed) ? parsed : null;
}

export async function loadAllSettingRows(): Promise<SettingRow[]> {
  const db = await getDB();
  return db.select<SettingRow[]>("SELECT key, value FROM settings");
}

export async function deleteSessionsBefore(cutoffTime: number): Promise<void> {
  await executeWriteBatch([
    {
      query: "DELETE FROM session_title_samples WHERE session_id IN (SELECT id FROM sessions WHERE start_time < ?)",
      values: [cutoffTime],
    },
    { query: "DELETE FROM sessions WHERE start_time < ?", values: [cutoffTime] },
    { query: "DELETE FROM web_activity_segments WHERE start_time < ?", values: [cutoffTime] },
    { query: "DELETE FROM import_exact_sessions WHERE start_time < ?", values: [cutoffTime] },
    { query: "DELETE FROM import_time_buckets WHERE bucket_start_time < ?", values: [cutoffTime] },
    { query: "UPDATE import_batches SET exact_session_count = (SELECT COUNT(*) FROM import_exact_sessions WHERE batch_id = import_batches.id), hour_bucket_count = (SELECT COUNT(*) FROM import_time_buckets WHERE batch_id = import_batches.id)" },
    { query: "DELETE FROM import_batches WHERE exact_session_count = 0 AND hour_bucket_count = 0" },
  ]);
}

export async function clearAllSessionWindowTitles(): Promise<void> {
  await executeWriteBatch([
    { query: "DELETE FROM session_title_samples" },
    { query: "UPDATE sessions SET window_title = '' WHERE COALESCE(window_title, '') <> ''" },
    { query: "UPDATE import_exact_sessions SET window_title = '' WHERE window_title <> ''" },
  ]);
}

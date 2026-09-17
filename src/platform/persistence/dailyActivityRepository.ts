import { invoke } from "@tauri-apps/api/core";

export interface DailyActivityRead {
  earliestStartTime: number | null;
  days: Array<{ date: string; duration: number }>;
}

function dateKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function integer(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value);
}

export async function getDailyActivity(
  startMs: number,
  endMs: number,
  request: (from: string, to: string) => Promise<unknown> = (from, to) => invoke("cmd_get_daily_activity", { from, to }),
): Promise<DailyActivityRead> {
  const boundaries = [startMs];
  const dates: string[] = [];
  let cursor = new Date(startMs);
  if (!integer(startMs) || !integer(endMs) || endMs <= startMs || !Number.isFinite(cursor.getTime())) {
    throw new Error("Invalid heatmap range");
  }
  while (cursor.getTime() < endMs && dates.length < 378) {
    if (cursor.getHours() || cursor.getMinutes() || cursor.getSeconds() || cursor.getMilliseconds()) {
      throw new Error("Heatmap range requires local midnights");
    }
    dates.push(dateKey(cursor));
    cursor = new Date(cursor.getFullYear(), cursor.getMonth(), cursor.getDate() + 1);
    boundaries.push(cursor.getTime());
  }
  if (cursor.getTime() !== endMs) throw new Error("Invalid heatmap day count");
  const value = await request(dates[0], dateKey(cursor));
  if (!record(value) || !integer(value.sampled_at_ms)
    || (value.earliest_start_ms !== null && !integer(value.earliest_start_ms))
    || !Array.isArray(value.days) || value.days.length !== dates.length) {
    throw new Error("Invalid daily activity response");
  }
  const days = value.days.map((day: unknown, index: number) => {
    if (!record(day) || day.start_ms !== boundaries[index] || day.end_ms !== boundaries[index + 1]
      || !integer(day.active_ms) || day.active_ms < 0) {
      throw new Error("Invalid daily activity boundary or duration; check runtime timezone");
    }
    return { date: dates[index], duration: day.active_ms };
  });
  return { earliestStartTime: value.earliest_start_ms as number | null, days };
}

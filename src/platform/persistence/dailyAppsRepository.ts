import { invoke } from "@tauri-apps/api/core";
import { getDailyActivity } from "./dailyActivityRepository.ts";

export interface DailyAppsRead {
  sampledAtMs: number;
  days: Array<{
    date: string;
    duration: number;
    apps: Array<{ appKey: string; duration: number }>;
  }>;
}

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export async function getDailyApps(
  startMs: number,
  endMs: number,
  request: (from: string, to: string) => Promise<unknown> = (from, to) => invoke("cmd_get_daily_apps", { from, to }),
): Promise<DailyAppsRead> {
  let sampledAtMs = 0;
  let applications: DailyAppsRead["days"][number]["apps"][] = [];
  // Reuse local midnight validation and the Desktop daily-read queue. This does
  // not infer intervals or classify canonical keys as display names.
  const daily = await getDailyActivity(startMs, endMs, async (from, to) => {
    const value = await request(from, to);
    if (!record(value) || !Number.isSafeInteger(value.sampled_at_ms)
      || !Array.isArray(value.days) || value.days.length > 378) {
      throw new Error("Invalid daily applications response");
    }
    sampledAtMs = value.sampled_at_ms as number;
    const keys = new Set<string>();
    let rowCount = 0;
    const encoder = new TextEncoder();
    applications = value.days.map((day: unknown) => {
      if (!record(day) || !Array.isArray(day.apps) || day.apps.length > 4096) {
        throw new Error("Invalid daily applications day");
      }
      let duration = 0;
      const seen = new Set<string>();
      const apps = day.apps.map((app: unknown) => {
        if (!record(app) || typeof app.app_key !== "string" || !app.app_key
          || encoder.encode(app.app_key).length > 1024 || seen.has(app.app_key)
          || typeof app.active_ms !== "number" || !Number.isSafeInteger(app.active_ms) || app.active_ms <= 0) {
          throw new Error("Invalid daily application total");
        }
        seen.add(app.app_key);
        keys.add(app.app_key);
        rowCount++;
        duration += app.active_ms;
        if (keys.size > 4096 || rowCount > 50_000 || !Number.isSafeInteger(duration)) {
          throw new Error("Daily applications response exceeds budget");
        }
        return { appKey: app.app_key, duration: app.active_ms };
      });
      if (duration !== day.active_ms) throw new Error("Daily application totals do not match the day");
      return apps;
    });
    return { ...value, earliest_start_ms: null };
  });
  return {
    sampledAtMs,
    days: daily.days.map((day, index) => ({ ...day, apps: applications[index] })),
  };
}

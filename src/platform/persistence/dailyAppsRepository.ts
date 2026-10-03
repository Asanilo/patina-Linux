import { invoke } from "@tauri-apps/api/core";
import { getDailyActivity } from "./dailyActivityRepository.ts";
import { isAppCategory, type AppCategory } from "../../shared/classification/categoryTokens.ts";
import { getUiTextLanguage } from "../../shared/copy/uiText.ts";

export interface DailyAppsRead {
  sampledAtMs: number;
  configurationRevision: string;
  trackingHealth: { status: "healthy" | "stale" | "unavailable"; lastHeartbeatMs: number | null; liveCutoffMs: number; staleAfterMs: number };
  applications: Array<{ appKey: string; appName: string; exeName: string; category: AppCategory; displayNameOverride: string | null }>;
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
  request: (from: string, to: string) => Promise<unknown> = (from, to) => invoke("cmd_get_daily_apps", { from, to, language: getUiTextLanguage() }),
): Promise<DailyAppsRead> {
  let sampledAtMs = 0;
  let configurationRevision = "";
  let trackingHealth: DailyAppsRead["trackingHealth"] | undefined;
  let applications: DailyAppsRead["days"][number]["apps"][] = [];
  let identities: DailyAppsRead["applications"] = [];
  // Reuse local midnight validation and the Desktop daily-read queue. This does
  // not infer intervals or classify canonical keys as display names.
  const daily = await getDailyActivity(startMs, endMs, async (from, to) => {
    const value = await request(from, to);
    if (!record(value) || !Number.isSafeInteger(value.sampled_at_ms)
      || typeof value.configuration_revision !== "string" || !/^[0-9a-f]{64}$/.test(value.configuration_revision)
      || !Array.isArray(value.days) || value.days.length > 378) {
      throw new Error("Invalid daily applications response");
    }
    sampledAtMs = value.sampled_at_ms as number;
    configurationRevision = value.configuration_revision;
    const health = value.tracking_health;
    if (!record(health) || !Number.isSafeInteger(health.live_cutoff_ms) || !Number.isSafeInteger(health.stale_after_ms)
      || (health.live_cutoff_ms as number) < 0 || (health.live_cutoff_ms as number) > sampledAtMs || (health.stale_after_ms as number) <= 0) {
      throw new Error("Invalid activity read health");
    }
    const heartbeat = health.last_heartbeat_ms;
    const hasHeartbeat = typeof heartbeat === "number" && Number.isSafeInteger(heartbeat) && heartbeat > 0 && heartbeat <= sampledAtMs;
    const valid = health.status === "unavailable" ? heartbeat === null && health.live_cutoff_ms === 0
      : health.status === "healthy" ? hasHeartbeat && sampledAtMs - (heartbeat as number) <= (health.stale_after_ms as number) && health.live_cutoff_ms === sampledAtMs
      : health.status === "stale" && hasHeartbeat && sampledAtMs - (heartbeat as number) > (health.stale_after_ms as number) && health.live_cutoff_ms === heartbeat;
    if (!valid) throw new Error("Inconsistent activity read health");
    trackingHealth = { status: health.status as DailyAppsRead["trackingHealth"]["status"], lastHeartbeatMs: heartbeat as number | null,
      liveCutoffMs: health.live_cutoff_ms as number, staleAfterMs: health.stale_after_ms as number };
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
    if (!Array.isArray(value.applications) || value.applications.length !== keys.size) {
      throw new Error("Daily application identities are missing; update the runtime");
    }
    const identityKeys = new Set<string>();
    identities = value.applications.map((identity: unknown) => {
      if (!record(identity) || typeof identity.app_key !== "string" || !keys.has(identity.app_key)
        || identityKeys.has(identity.app_key) || typeof identity.app_name !== "string"
        || typeof identity.exe_name !== "string" || !identity.exe_name
        || typeof identity.category !== "string" || !isAppCategory(identity.category) || identity.category === "system"
        || encoder.encode(identity.category).length > 1024
        || !(identity.display_name_override === null || (typeof identity.display_name_override === "string"
          && identity.display_name_override.length > 0 && encoder.encode(identity.display_name_override).length <= 4096))
        || encoder.encode(identity.app_name).length > 1024 || encoder.encode(identity.exe_name).length > 1024) {
        throw new Error("Invalid daily application identity");
      }
      identityKeys.add(identity.app_key);
      return { appKey: identity.app_key, appName: identity.app_name, exeName: identity.exe_name,
        category: identity.category, displayNameOverride: identity.display_name_override };
    });
    return { ...value, earliest_start_ms: null };
  });
  if (!trackingHealth) throw new Error("Missing activity read health");
  return {
    sampledAtMs,
    configurationRevision,
    trackingHealth,
    applications: identities,
    days: daily.days.map((day, index) => ({ ...day, apps: applications[index] })),
  };
}

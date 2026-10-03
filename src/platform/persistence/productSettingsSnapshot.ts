import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../../shared/settings/appSettings.ts";

export interface ProductSettingsSnapshot {
  revision: string;
  settings: Pick<AppSettings, "idleTimeoutSecs" | "timelineMergeGapSecs" | "minSessionSecs"
    | "trackingPaused" | "audioParticipationEnabled" | "webActivityEnabled"
    | "webActivityPort" | "webActivityUrlPrivacy">;
  lastHeartbeatMs: number | null;
  lastSuccessfulSampleMs: number | null;
}

export function parseProductSettingsSnapshot(raw: unknown): ProductSettingsSnapshot {
  const fail = () => new Error("Invalid product settings snapshot");
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw fail();
  const value = raw as Record<string, unknown>;
  if (typeof value.revision !== "string" || !/^[a-f0-9]{64}$/.test(value.revision)
    || !Number.isSafeInteger(value.sampled_at_ms)
    || !value.settings || typeof value.settings !== "object" || Array.isArray(value.settings)
    || new TextEncoder().encode(JSON.stringify(raw)).length > 8192) throw fail();
  const settings = value.settings as Record<string, unknown>;
  const integer = (key: string, min = 0, max = Number.MAX_SAFE_INTEGER) => {
    const v = settings[key];
    if (typeof v !== "number" || !Number.isSafeInteger(v) || v < min || v > max) throw fail();
    return v;
  };
  const boolean = (key: string) => {
    const v = settings[key];
    if (typeof v !== "boolean") throw fail();
    return v;
  };
  const timestamp = (key: string) => {
    const v = value[key];
    if (v === null) return null;
    if (typeof v !== "number" || !Number.isSafeInteger(v) || v < 0) throw fail();
    return v;
  };
  const privacy = settings.web_activity_url_privacy;
  if (privacy !== "full" && privacy !== "strip_query" && privacy !== "domain_only") throw fail();
  const minSessionSecs = integer("min_session_secs", 60, 600);
  if (minSessionSecs % 60 !== 0) throw fail();
  const enabled = boolean("web_activity_enabled");
  const tokenPresent = boolean("web_activity_token_present");
  if (enabled && !tokenPresent) throw fail();
  return {
    revision: value.revision,
    settings: {
      idleTimeoutSecs: integer("idle_timeout_secs"),
      timelineMergeGapSecs: integer("timeline_merge_gap_secs"),
      minSessionSecs,
      trackingPaused: boolean("tracking_paused"),
      audioParticipationEnabled: boolean("audio_participation_enabled"),
      webActivityEnabled: enabled,
      webActivityPort: integer("web_activity_port", 1024, 65535),
      webActivityUrlPrivacy: privacy,
    },
    lastHeartbeatMs: timestamp("last_heartbeat_ms"),
    lastSuccessfulSampleMs: timestamp("last_successful_sample_ms"),
  };
}

export async function loadProductSettingsSnapshot(): Promise<ProductSettingsSnapshot> {
  return parseProductSettingsSnapshot(await invoke<unknown>("cmd_get_product_settings"));
}

import { invoke } from "@tauri-apps/api/core";
import type { AppSettings } from "../../shared/settings/appSettings.ts";
import type { ProductSettingsSnapshot as RawProductSettingsSnapshot } from "../protocol/protocol.generated.ts";

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
  const validated: RawProductSettingsSnapshot = {
    revision: value.revision,
    sampled_at_ms: value.sampled_at_ms as number,
    settings: {
      idle_timeout_secs: integer("idle_timeout_secs"),
      timeline_merge_gap_secs: integer("timeline_merge_gap_secs"),
      min_session_secs: minSessionSecs,
      tracking_paused: boolean("tracking_paused"),
      audio_participation_enabled: boolean("audio_participation_enabled"),
      web_activity_enabled: enabled,
      web_activity_token_present: tokenPresent,
      web_activity_port: integer("web_activity_port", 1024, 65535),
      web_activity_url_privacy: privacy,
    },
    last_heartbeat_ms: timestamp("last_heartbeat_ms"),
    last_successful_sample_ms: timestamp("last_successful_sample_ms"),
  };
  return {
    revision: validated.revision,
    settings: {
      idleTimeoutSecs: validated.settings.idle_timeout_secs,
      timelineMergeGapSecs: validated.settings.timeline_merge_gap_secs,
      minSessionSecs: validated.settings.min_session_secs,
      trackingPaused: validated.settings.tracking_paused,
      audioParticipationEnabled: validated.settings.audio_participation_enabled,
      webActivityEnabled: validated.settings.web_activity_enabled,
      webActivityPort: validated.settings.web_activity_port,
      webActivityUrlPrivacy: validated.settings.web_activity_url_privacy,
    },
    lastHeartbeatMs: validated.last_heartbeat_ms,
    lastSuccessfulSampleMs: validated.last_successful_sample_ms,
  };
}

export async function loadProductSettingsSnapshot(): Promise<ProductSettingsSnapshot> {
  return parseProductSettingsSnapshot(await invoke<unknown>("cmd_get_product_settings"));
}

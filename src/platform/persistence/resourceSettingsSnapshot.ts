import { invoke } from "@tauri-apps/api/core";
import type { ResourceSettingsSnapshot as RawResourceSettingsSnapshot } from "../protocol/protocol.generated.ts";
import type { AppSettings } from "../../shared/settings/appSettings.ts";

export interface ResourceSettingsSnapshot {
  revision: string;
  settings: Pick<AppSettings, "audioParticipationEnabled" | "webActivityEnabled" | "webActivityPort" | "webActivityUrlPrivacy">;
}

export function parseResourceSettingsSnapshot(raw: unknown): ResourceSettingsSnapshot {
  const fail = () => new Error("Invalid resource settings snapshot");
  if (!raw || typeof raw !== "object" || Array.isArray(raw)) throw fail();
  const value = raw as Record<string, unknown>;
  if (typeof value.revision !== "string" || !/^[a-f0-9]{64}$/.test(value.revision)
    || typeof value.sampled_at_ms !== "number" || !Number.isSafeInteger(value.sampled_at_ms) || value.sampled_at_ms < 0
    || typeof value.audio_participation_enabled !== "boolean"
    || !value.browser_activity || typeof value.browser_activity !== "object" || Array.isArray(value.browser_activity)
    || new TextEncoder().encode(JSON.stringify(raw)).length > 8192) throw fail();
  const browser = value.browser_activity as Record<string, unknown>;
  if (typeof browser.enabled !== "boolean" || typeof browser.token_present !== "boolean"
    || (browser.enabled && !browser.token_present) || typeof browser.port !== "number"
    || !Number.isSafeInteger(browser.port) || browser.port < 1024 || browser.port > 65535
    || !["full", "strip_query", "domain_only"].includes(String(browser.url_privacy))) throw fail();
  const privacy = browser.url_privacy;
  if (privacy !== "full" && privacy !== "strip_query" && privacy !== "domain_only") throw fail();
  const validated: RawResourceSettingsSnapshot = {revision: value.revision, sampled_at_ms: value.sampled_at_ms,
    audio_participation_enabled: value.audio_participation_enabled,
    browser_activity: {enabled: browser.enabled, port: browser.port, token_present: browser.token_present, url_privacy: privacy}};
  return {revision: validated.revision, settings: {
    audioParticipationEnabled: validated.audio_participation_enabled,
    webActivityEnabled: validated.browser_activity.enabled,
    webActivityPort: validated.browser_activity.port,
    webActivityUrlPrivacy: validated.browser_activity.url_privacy,
  }};
}

export async function loadResourceSettingsSnapshot(): Promise<ResourceSettingsSnapshot | null> {
  const raw = await invoke<unknown>("cmd_get_resource_settings");
  // Only the host's explicit null marks embedded compatibility; errors never fallback.
  return raw === null ? null : parseResourceSettingsSnapshot(raw);
}

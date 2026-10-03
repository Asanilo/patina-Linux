import { invoke } from "@tauri-apps/api/core";
import { getDB } from "./sqlite.ts";

export interface SettingRow {
  key: string;
  value: string;
}

// Explicit legacy Desktop/host storage exception. Shared product policy is read
// through the daemon snapshot; new clients must not consume these preferences.
const DESKTOP_KEYS = [
  "refresh_interval_secs", "close_behavior", "minimize_behavior", "theme_mode", "language",
  "hourly_activity_chart_mode", "color_scheme_light", "color_scheme_dark", "launch_at_login",
  "background_tracking_at_login", "start_minimized", "background_optimization",
  "background_optimization_delay_minutes", "onboarding_completed", "web_activity_token",
  "local_api_port", "local_api_token", "remote_status_bridge_enabled", "remote_status_bridge_url",
  "remote_status_bridge_token", "remote_status_bridge_machine_id",
];
const REMOTE_BACKUP_KEYS = ["webdav_backup_url", "webdav_backup_username", "webdav_backup_remote_dir", "webdav_backup_last_backup_at_ms"];

async function loadKnownRows(keys: string[]): Promise<SettingRow[]> {
  const db = await getDB();
  return db.select<SettingRow[]>(`SELECT key, value FROM settings WHERE key IN (${keys.map(() => "?").join(",")})`, keys);
}

export const loadDesktopSettingRows = () => loadKnownRows(DESKTOP_KEYS);
export const loadRemoteBackupSettingRows = () => loadKnownRows(REMOTE_BACKUP_KEYS);

export async function deleteSessionsBefore(cutoffTime: number): Promise<void> {
  await invoke("cmd_delete_tracking_data_before", { cutoffTimeMs: cutoffTime });
}

export async function clearAllSessionWindowTitles(): Promise<void> {
  await invoke("cmd_clear_all_window_titles");
}

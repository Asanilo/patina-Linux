import { invoke } from "@tauri-apps/api/core";
import { getDB } from "./sqlite.ts";
import { CLASSIFICATION_PREFIXES, isClassificationKey, loadClassificationSnapshot } from "./classificationSnapshot.ts";

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

export interface SessionExeNameRow {
  exeName: string;
}

export interface ObservedSessionStatRow {
  exeName: string;
  appName: string;
  totalDuration: number;
  lastSeenMs: number;
}

export async function loadSettingValue(key: string): Promise<string | null> {
  if (!isClassificationKey(key)) throw new Error("Unsupported classification configuration key");
  return (await loadClassificationSnapshot()).entries.find((entry) => entry.key === key)?.value ?? null;
}

export async function loadSettingRowsByKeyPrefix(keyPrefix: string): Promise<SettingKeyValueRow[]> {
  if (!CLASSIFICATION_PREFIXES.some((prefix) => prefix === keyPrefix)) throw new Error("Unsupported classification namespace");
  return (await loadClassificationSnapshot()).entries.filter((entry) => entry.key.startsWith(keyPrefix));
}

export async function loadSettingKeysByKeyPrefix(keyPrefix: string): Promise<SettingKeyRow[]> {
  return (await loadSettingRowsByKeyPrefix(keyPrefix)).map(({ key }) => ({ key }));
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

export async function deleteSessionsByExeNames(exeNames: string[]): Promise<void> {
  if (exeNames.length === 0) {
    return;
  }
  await invoke("cmd_delete_app_tracking_data", {
    exeNames,
    startTimeMs: null,
    endTimeMs: null,
  });
}

export async function deleteSessionsByExeNamesBetween(
  exeNames: string[],
  startTime: number,
  endTime: number,
): Promise<void> {
  if (exeNames.length === 0) {
    return;
  }
  await invoke("cmd_delete_app_tracking_data", {
    exeNames,
    startTimeMs: startTime,
    endTimeMs: endTime,
  });
}

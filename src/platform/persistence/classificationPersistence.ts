import { invoke } from "@tauri-apps/api/core";
import { CLASSIFICATION_PREFIXES, isClassificationKey, loadClassificationSnapshot } from "./classificationSnapshot.ts";

export interface SettingKeyValueRow {
  key: string;
  value: string;
}

export interface SettingKeyRow {
  key: string;
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

/** Called only after the existing classification-page delete confirmation. */
export async function deleteCanonicalAppHistory(appKey: string, scope: "all" | "today"): Promise<number> {
  const raw = await invoke<unknown>("cmd_delete_canonical_app_history", {
    request: {app_key: appKey, scope, confirmed: true},
  });
  if (!raw || typeof raw !== "object") throw new Error("Invalid application cleanup result");
  const value = raw as Record<string,unknown>;
  if (typeof value.app_key !== "string" || !value.app_key || typeof value.matched_executables !== "number"
    || !Number.isSafeInteger(value.matched_executables) || value.matched_executables < 0 || value.matched_executables > 4096
    || !value.deleted || typeof value.deleted !== "object") throw new Error("Invalid application cleanup result");
  const deleted = value.deleted as Record<string,unknown>;
  for (const name of ["sessions_deleted","imported_exact_sessions_deleted","imported_time_buckets_deleted","import_batches_deleted"]) {
    const count = deleted[name];
    if (typeof count !== "number" || !Number.isSafeInteger(count) || count < 0) throw new Error("Invalid application cleanup count");
  }
  return value.matched_executables;
}

import { invoke } from "@tauri-apps/api/core";
import type { ObservedSessionStatRow } from "./classificationPersistence.ts";

export async function loadRecentObservedSessionStats(
  fromMs: number,
  toMs: number,
  request: (from: number, to: number) => Promise<unknown> = (from, to) => invoke("cmd_get_observed_apps", { fromMs: from, toMs: to }),
): Promise<ObservedSessionStatRow[]> {
  if (!Number.isSafeInteger(fromMs) || !Number.isSafeInteger(toMs)
    || fromMs < 0 || toMs <= fromMs || toMs - fromMs > 366 * 86400000) {
    throw new Error("Invalid observed apps range");
  }
  const rows = await request(fromMs, toMs);
  if (!Array.isArray(rows) || rows.length > 4096) throw new Error("Invalid observed apps response");
  const encoder = new TextEncoder();
  if (encoder.encode(JSON.stringify({ data: rows })).length > 1024 * 1024) throw new Error("Observed apps response exceeds budget");
  const seen = new Set<string>();
  return rows.map((row: unknown) => {
    if (row === null || typeof row !== "object") throw new Error("Invalid observed app");
    const item = row as Record<string, unknown>;
    if (typeof item.exe_name !== "string" || typeof item.app_name !== "string"
      || encoder.encode(item.exe_name).length > 1024 || encoder.encode(item.app_name).length > 1024
      || !Number.isSafeInteger(item.total_duration_ms) || (item.total_duration_ms as number) < 0
      || !Number.isSafeInteger(item.last_seen_ms) || (item.last_seen_ms as number) < fromMs || (item.last_seen_ms as number) >= toMs
      || seen.has(item.exe_name)) throw new Error("Invalid observed app");
    seen.add(item.exe_name);
    return { exeName: item.exe_name, appName: item.app_name,
      totalDuration: item.total_duration_ms as number, lastSeenMs: item.last_seen_ms as number };
  });
}

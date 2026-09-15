import { invoke } from "@tauri-apps/api/core";

interface RawProcessResourceSnapshot {
  handle_count: number | null;
  thread_count: number | null;
  working_set_bytes: number | null;
  private_usage_bytes: number | null;
  rss_bytes: number | null;
  pss_bytes: number | null;
  uss_bytes: number | null;
  swap_bytes: number | null;
}

interface RawProcessDetailsCacheStats {
  entries: number;
  positive_entries: number;
  negative_entries: number;
}

interface RawIconResultCacheStats {
  entries: number;
  positive_entries: number;
  negative_entries: number;
}

interface RawResourceDiagnosticsSnapshot {
  webview_window_count: number;
  webview_window_labels: string[];
  process_resources: RawProcessResourceSnapshot;
  process_details_cache: RawProcessDetailsCacheStats;
  icon_result_cache: RawIconResultCacheStats;
}

export interface ResourceDiagnosticsSnapshot {
  webviewWindowCount: number;
  webviewWindowLabels: string[];
  processResources: {
    handleCount: number | null;
    threadCount: number | null;
    workingSetBytes: number | null;
    privateUsageBytes: number | null;
    rssBytes: number | null;
    pssBytes: number | null;
    ussBytes: number | null;
    swapBytes: number | null;
  };
  processDetailsCache: {
    entries: number;
    positiveEntries: number;
    negativeEntries: number;
  };
  iconResultCache: {
    entries: number;
    positiveEntries: number;
    negativeEntries: number;
  };
}

declare global {
  interface Window {
    __TIME_TRACKER_RESOURCE_DIAGNOSTICS__?: () => Promise<ResourceDiagnosticsSnapshot>;
  }
}

function isNumber(value: unknown): value is number {
  return typeof value === "number" && Number.isFinite(value);
}

function isNullableNumber(value: unknown): value is number | null {
  return value === null || isNumber(value);
}

function isStringArray(value: unknown): value is string[] {
  return Array.isArray(value) && value.every((item) => typeof item === "string");
}

function isRawProcessResources(value: unknown): value is RawProcessResourceSnapshot {
  if (!value || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return isNullableNumber(record.handle_count)
    && isNullableNumber(record.thread_count)
    && isNullableNumber(record.working_set_bytes)
    && isNullableNumber(record.private_usage_bytes)
    && isNullableNumber(record.rss_bytes)
    && isNullableNumber(record.pss_bytes)
    && isNullableNumber(record.uss_bytes)
    && isNullableNumber(record.swap_bytes);
}

function isRawCacheStats(value: unknown): value is RawProcessDetailsCacheStats {
  if (!value || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return isNumber(record.entries)
    && isNumber(record.positive_entries)
    && isNumber(record.negative_entries);
}

function isRawResourceDiagnostics(value: unknown): value is RawResourceDiagnosticsSnapshot {
  if (!value || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return isNumber(record.webview_window_count)
    && isStringArray(record.webview_window_labels)
    && isRawProcessResources(record.process_resources)
    && isRawCacheStats(record.process_details_cache)
    && isRawCacheStats(record.icon_result_cache);
}

function mapRawCacheStats(raw: RawProcessDetailsCacheStats) {
  return {
    entries: raw.entries,
    positiveEntries: raw.positive_entries,
    negativeEntries: raw.negative_entries,
  };
}

function mapRawResourceDiagnostics(raw: RawResourceDiagnosticsSnapshot): ResourceDiagnosticsSnapshot {
  return {
    webviewWindowCount: raw.webview_window_count,
    webviewWindowLabels: raw.webview_window_labels,
    processResources: {
      handleCount: raw.process_resources.handle_count,
      threadCount: raw.process_resources.thread_count,
      workingSetBytes: raw.process_resources.working_set_bytes,
      privateUsageBytes: raw.process_resources.private_usage_bytes,
      rssBytes: raw.process_resources.rss_bytes,
      pssBytes: raw.process_resources.pss_bytes,
      ussBytes: raw.process_resources.uss_bytes,
      swapBytes: raw.process_resources.swap_bytes,
    },
    processDetailsCache: mapRawCacheStats(raw.process_details_cache),
    iconResultCache: mapRawCacheStats(raw.icon_result_cache),
  };
}

export async function loadResourceDiagnostics(): Promise<ResourceDiagnosticsSnapshot> {
  const payload = await invoke<unknown>("cmd_get_resource_diagnostics");
  if (!isRawResourceDiagnostics(payload)) {
    throw new Error("Invalid resource diagnostics payload");
  }

  return mapRawResourceDiagnostics(payload);
}

export function installDevelopmentResourceDiagnostics() {
  if (!import.meta.env.DEV || typeof window === "undefined") {
    return;
  }

  window.__TIME_TRACKER_RESOURCE_DIAGNOSTICS__ = loadResourceDiagnostics;
}

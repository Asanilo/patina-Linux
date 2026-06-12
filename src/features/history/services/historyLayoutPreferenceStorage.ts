import type { HistoryTimelineDisplayMode } from "./historyTimelineViewModel.ts";

const HISTORY_TIMELINE_MODE_KEY = "patina:history-timeline-mode";
const LEGACY_HISTORY_TIMELINE_MODE_KEY = "time-tracker:history-timeline-mode";
const HISTORY_DAY_DISTRIBUTION_MODE_KEY = "patina:history-day-distribution-mode";
const LEGACY_HISTORY_DAY_DISTRIBUTION_MODE_KEY = "time-tracker:history-day-distribution-mode";

export type DayDistributionMode = "app" | "category";

function getStorage(): Storage | null {
  if (typeof window === "undefined") return null;
  return window.localStorage;
}

function isDayDistributionMode(value: string | null): value is DayDistributionMode {
  return value === "app" || value === "category";
}

function isHistoryTimelineMode(value: string | null): value is HistoryTimelineDisplayMode {
  return value === "app" || value === "category";
}

export function readHistoryTimelineMode(): HistoryTimelineDisplayMode {
  const storage = getStorage();
  if (!storage) return "app";

  try {
    const value = storage.getItem(HISTORY_TIMELINE_MODE_KEY);
    if (isHistoryTimelineMode(value)) return value;

    const legacyValue = storage.getItem(LEGACY_HISTORY_TIMELINE_MODE_KEY);
    if (!isHistoryTimelineMode(legacyValue)) return "app";

    storage.setItem(HISTORY_TIMELINE_MODE_KEY, legacyValue);
    storage.removeItem(LEGACY_HISTORY_TIMELINE_MODE_KEY);
    return legacyValue;
  } catch {
    return "app";
  }
}

export function rememberHistoryTimelineMode(mode: HistoryTimelineDisplayMode) {
  const storage = getStorage();
  if (!storage) return;

  try {
    storage.setItem(HISTORY_TIMELINE_MODE_KEY, mode);
    storage.removeItem(LEGACY_HISTORY_TIMELINE_MODE_KEY);
  } catch {
    // History layout preferences are best-effort; never block the interaction.
  }
}

export function readHistoryDayDistributionMode(): DayDistributionMode {
  const storage = getStorage();
  if (!storage) return "app";

  try {
    const value = storage.getItem(HISTORY_DAY_DISTRIBUTION_MODE_KEY);
    if (isDayDistributionMode(value)) return value;

    const legacyValue = storage.getItem(LEGACY_HISTORY_DAY_DISTRIBUTION_MODE_KEY);
    if (!isDayDistributionMode(legacyValue)) return "app";

    storage.setItem(HISTORY_DAY_DISTRIBUTION_MODE_KEY, legacyValue);
    storage.removeItem(LEGACY_HISTORY_DAY_DISTRIBUTION_MODE_KEY);
    return legacyValue;
  } catch {
    return "app";
  }
}

export function rememberHistoryDayDistributionMode(mode: DayDistributionMode) {
  const storage = getStorage();
  if (!storage) return;

  try {
    storage.setItem(HISTORY_DAY_DISTRIBUTION_MODE_KEY, mode);
    storage.removeItem(LEGACY_HISTORY_DAY_DISTRIBUTION_MODE_KEY);
  } catch {
    // History layout preferences are best-effort; never block the interaction.
  }
}

import {
  loadAppSettingsSnapshot,
  loadTrackerHealthTimestamp,
  saveAppSetting,
  saveAppSettingsPatch,
  type AppSettings,
} from "../../platform/persistence/appSettingsStore.ts";
import { loadProductSettingsSnapshot } from "../../platform/persistence/productSettingsSnapshot.ts";
import type { HourlyActivityChartMode } from "../../shared/settings/appSettings.ts";
import {
  onAppSettingsChanged,
  type AppSettingsChangedPayload,
} from "../../platform/runtime/appSettingsEventGateway.ts";

export type { AppSettings };
export type AppSettingsReadSnapshot = Awaited<ReturnType<typeof loadAppSettingsSnapshot>>;
export const loadCurrentAppSettingsSnapshot = loadAppSettingsSnapshot;

export async function subscribeAppSettingsChanged(
  handler: (payload: AppSettingsChangedPayload) => void | Promise<void>,
): Promise<() => void> {
  return onAppSettingsChanged(handler);
}

export async function loadLatestTrackingPauseSetting(): Promise<boolean> {
  return (await loadProductSettingsSnapshot()).settings.trackingPaused;
}

export async function loadTrackerHealthTimestampMs(): Promise<number | null> {
  return loadTrackerHealthTimestamp();
}

export async function saveMinSessionSecsSetting(nextValue: number, expectedRevision: string) {
  if (!/^[a-f0-9]{64}$/.test(expectedRevision)) throw new Error("Product settings baseline is unavailable");
  const confirmation = await saveAppSettingsPatch({minSessionSecs: nextValue}, expectedRevision);
  if (!confirmation) throw new Error("Product settings confirmation is missing");
  return confirmation;
}

export async function saveHourlyActivityChartModeSetting(
  nextValue: HourlyActivityChartMode,
): Promise<void> {
  await saveAppSetting("hourlyActivityChartMode", nextValue);
}

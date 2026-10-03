import { subscribeTrackingDataChanged } from "./appRuntimeTrackingService.ts";

export async function loadAppIcons(signal: AbortSignal): Promise<Record<string,string>> {
  const {getIconMap} = await import("../../platform/persistence/iconRepository.ts");
  return getIconMap(undefined, signal);
}

export function subscribeAppIconInvalidation(invalidate: () => void): Promise<() => void> {
  return subscribeTrackingDataChanged(payload => {
    if (payload.reason === "backup-restored" || payload.reason === "daemon-client-resync") invalidate();
  });
}

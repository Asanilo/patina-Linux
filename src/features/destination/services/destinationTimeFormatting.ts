import { getUiLocale } from "../../../shared/copy/uiText.ts";

export function formatDestinationDuration(durationMs: number) {
  const safeMs = Math.max(0, durationMs);
  const totalSeconds = Math.floor(safeMs / 1_000);
  const totalMinutes = Math.floor(safeMs / 60_000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  if (hours > 0) return `${hours}h ${minutes}m`;
  if (totalMinutes > 0) return `${totalMinutes}m`;
  if (totalSeconds > 0) return `${totalSeconds}s`;
  return "0m";
}

export function formatDestinationTime(timeMs: number, dayEndMs?: number) {
  if (dayEndMs !== undefined && timeMs === dayEndMs) return "24:00";
  return new Date(timeMs).toLocaleTimeString(getUiLocale(), {
    hour: "2-digit",
    minute: "2-digit",
    hourCycle: "h23",
  });
}

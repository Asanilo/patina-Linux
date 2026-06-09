export function formatMinuteInput(value: number): string {
  return String(value);
}

export function parseBoundedMinuteInput(
  value: string,
  minMinutes: number,
  maxMinutes: number,
): number | null {
  const trimmed = value.trim();
  if (!trimmed) return null;

  const parsed = Number(trimmed);
  if (!Number.isFinite(parsed)) return null;

  const rounded = Math.round(parsed);
  if (rounded < minMinutes || rounded > maxMinutes) return null;

  return rounded;
}

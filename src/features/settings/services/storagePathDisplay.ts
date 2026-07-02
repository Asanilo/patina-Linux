const UNITS = ["B", "KiB", "MiB", "GiB", "TiB"] as const;

export function formatStorageBytes(value: number): string {
  if (!Number.isFinite(value) || value <= 0) return "0 B";
  let amount = value;
  let unitIndex = 0;
  while (amount >= 1_024 && unitIndex < UNITS.length - 1) {
    amount /= 1_024;
    unitIndex += 1;
  }
  if (unitIndex === 0) return `${Math.round(amount)} B`;
  return `${amount.toFixed(1)} ${UNITS[unitIndex]}`;
}

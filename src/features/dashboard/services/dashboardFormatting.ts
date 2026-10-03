import type { AppCategory } from "../../../shared/classification/categoryTokens.ts";

export interface CategoryDistItem {
  category: AppCategory;
  name: string;
  value: number;
  color: string;
}

export interface TopApplicationItem {
  exeName: string;
  name: string;
  color: string;
  duration: number;
  percentage: number;
  categoryInitial: string;
}

export function formatDashboardDuration(ms: number) {
  const safeMs = Math.max(0, ms);
  const totalMinutes = Math.floor(safeMs / 60000);
  const hours = Math.floor(totalMinutes / 60);
  const minutes = totalMinutes % 60;

  if (hours > 0) return `${hours}h ${minutes}m`;
  return `${minutes}m`;
}

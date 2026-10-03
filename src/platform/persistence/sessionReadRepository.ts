import type { HistorySession } from "../../shared/types/sessions.ts";

export async function getSessionsInRange(startMs: number, endMs: number): Promise<HistorySession[]> {
  const { getExactHistorySnapshot } = await import("./historyRepository.ts");
  return (await getExactHistorySnapshot(startMs, endMs)).sessions;
}

export async function getHistoryByDate(date: Date): Promise<HistorySession[]> {
  const start = new Date(date);
  start.setHours(0, 0, 0, 0);
  const end = new Date(date);
  end.setHours(24, 0, 0, 0);
  return getSessionsInRange(start.getTime(), end.getTime());
}

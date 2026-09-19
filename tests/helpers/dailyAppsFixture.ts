import type { AggregateSessionRecord } from "../../src/platform/persistence/sessionReadRepository.ts";
import type { DailyAppsRead } from "../../src/platform/persistence/dailyAppsRepository.ts";
import { AppClassification } from "../../src/shared/classification/appClassification.ts";

export function dailyAppsFixture(sessions: AggregateSessionRecord[]): DailyAppsRead {
  const applications = new Map<string, DailyAppsRead["applications"][number]>();
  const days = new Map<string, Map<string, number>>();
  for (const session of sessions) {
    const key = AppClassification.resolveCanonicalExecutable(session.exeName);
    if (!applications.has(key)) applications.set(key, { appKey: key, appName: session.appName, exeName: AppClassification.normalizeExecutable(session.exeName) === key ? session.exeName : key });
    for (let start = session.startTime;start < session.endTime;) {
      const date = new Date(start);
      const next = new Date(date.getFullYear(), date.getMonth(), date.getDate() + 1).getTime();
      const dateKey = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
      const values = days.get(dateKey) ?? new Map<string, number>();
      values.set(key, (values.get(key) ?? 0) + Math.min(next, session.endTime) - start);
      days.set(dateKey, values);
      start = next;
    }
  }
  return {
    sampledAtMs: Date.now(), applications: [...applications.values()], days: [...days].map(([date, values]) => ({
      date, duration: [...values.values()].reduce((a, b) => a + b, 0), apps: [...values].map(([appKey, duration]) => ({ appKey, duration })),
    }))
  };
}

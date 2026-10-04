// Historical replay reference only. Production clients consume backend hour quantities.
export * from "../../src/shared/lib/hourlyActivityCompiler.ts";
import { buildHourlyCategoryPresentation, type HourlyActivityPoint, type HourlyCategoryActivity } from "../../src/shared/lib/hourlyActivityCompiler.ts";
import { AppClassification } from "../../src/shared/classification/appClassification.ts";
import type { AppCategory } from "../../src/shared/classification/categoryTokens.ts";
import type { HistorySession } from "../../src/shared/types/sessions.ts";
import { getSessionCategory } from "../../src/shared/lib/sessionReadCompiler.ts";
interface CategoryDescriptor { category: AppCategory; name: string; color: string }
function formatHourlyDisplayMinutes(minutes: number) { return !Number.isFinite(minutes) || minutes < 1 ? 0 : Math.round(minutes); }
function forEachHourlySessionSegment(
  session: HistorySession,
  visit: (hourIndex: number, durationMs: number) => void,
) {
  const start = new Date(session.startTime);
  const end = session.endTime !== null ? new Date(session.endTime) : new Date();
  let currentPtr = start.getTime();

  while (currentPtr < end.getTime()) {
    const currentDate = new Date(currentPtr);
    const hourIndex = currentDate.getHours();
    const nextHour = new Date(currentPtr);
    nextHour.setHours(hourIndex + 1, 0, 0, 0);

    const segmentEnd = Math.min(end.getTime(), nextHour.getTime());
    visit(hourIndex, segmentEnd - currentPtr);
    currentPtr = segmentEnd;
  }
}

export function buildHourlyActivity(sessions: HistorySession[]): HourlyActivityPoint[] {
  const hoursCount = new Array<number>(24).fill(0);

  for (const session of sessions) {
    forEachHourlySessionSegment(session, (hourIndex, durationMs) => {
      hoursCount[hourIndex] += durationMs / 60000;
    });
  }

  return hoursCount.map((minutes, hourIndex) => ({
    hour: `${hourIndex.toString().padStart(2, "0")}:00`,
    minutes: formatHourlyDisplayMinutes(minutes),
  }));
}

function incrementCategoryMinutes(
  bucket: Map<AppCategory, number>,
  category: AppCategory,
  minutes: number,
) {
  bucket.set(category, (bucket.get(category) ?? 0) + minutes);
}

export function buildHourlyCategoryActivity(
  sessions: HistorySession[],
): HourlyCategoryActivity {
  const hourlyCategoryMinutes = Array.from({ length: 24 }, () => new Map<AppCategory, number>());
  const categoryTotals = new Map<AppCategory, number>();
  const categoryDescriptors = new Map<AppCategory, CategoryDescriptor>();
  const appCategoryCache = new Map<string, CategoryDescriptor>();

  for (const session of sessions) {
    const cacheKey = JSON.stringify([session.exeName, session.appName, session.confirmed?.category ?? null]);
    let descriptor = appCategoryCache.get(cacheKey);
    if (!descriptor) {
      const category = getSessionCategory(session);
      descriptor = {
        category,
        name: AppClassification.getCategoryLabel(category),
        color: AppClassification.getCategoryColor(category),
      };
      appCategoryCache.set(cacheKey, descriptor);
      categoryDescriptors.set(category, descriptor);
    }

    forEachHourlySessionSegment(session, (hourIndex, durationMs) => {
      const minutes = durationMs / 60000;
      incrementCategoryMinutes(hourlyCategoryMinutes[hourIndex], descriptor.category, minutes);
      incrementCategoryMinutes(categoryTotals, descriptor.category, minutes);
    });
  }

  return buildHourlyCategoryPresentation(hourlyCategoryMinutes, categoryDescriptors, categoryTotals);
}


import { buildHistoryReadModel as buildCurrentHistoryReadModel } from "../../src/features/history/services/historyReadModel.ts";
/** Replay-only old projection; never imported by a shipped client. */
export function buildLegacyHistoryReadModel(params: Omit<Parameters<typeof buildCurrentHistoryReadModel>[0], "hours">) {
  const view = buildCurrentHistoryReadModel({...params,hours:null});
  return {...view,hourlyActivity:buildHourlyActivity(view.compiledSessions),hourlyCategoryActivity:buildHourlyCategoryActivity(view.compiledSessions)};
}

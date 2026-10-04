import type { ActivityHour as RawActivityHour } from "../protocol/protocol.generated.ts";
import type { AppCategory } from "../../shared/classification/categoryTokens.ts";
import type { ActivityHour } from "../../shared/types/activityHours.ts";
export type ActivityHourRead = ActivityHour;

/** Validate quantities, never recompute their clock placement in a client timezone. */
export function parseActivityHours(raw: unknown, expected: Map<AppCategory, number>): ActivityHourRead[] {
  const fail = () => new Error("Invalid activity hourly projection");
  if (!Array.isArray(raw) || raw.length !== 24 || expected.size > 4096) throw fail();
  const actual = new Map<AppCategory, number>();
  const hours = raw.map((value: unknown, hour: number): ActivityHourRead => {
    if (!value || typeof value !== "object" || Array.isArray(value)) throw fail();
    const entry = value as Record<string, unknown>;
    if (entry.hour !== hour || !Array.isArray(entry.categories) || entry.categories.length > 4096) throw fail();
    let total = 0;
    const seen = new Set<string>();
    const categories = entry.categories.map((item: unknown) => {
      if (!item || typeof item !== "object" || Array.isArray(item)) throw fail();
      const category = item as Record<string, unknown>;
      if (typeof category.category !== "string" || !expected.has(category.category as AppCategory)
        || seen.has(category.category) || typeof category.active_ms !== "number"
        || !Number.isSafeInteger(category.active_ms) || category.active_ms <= 0) throw fail();
      seen.add(category.category);
      const key = category.category as AppCategory;
      total += category.active_ms;
      actual.set(key, (actual.get(key) ?? 0) + category.active_ms);
      if (!Number.isSafeInteger(total) || !Number.isSafeInteger(actual.get(key))) throw fail();
      return {category: key, duration: category.active_ms};
    });
    if (entry.active_ms !== total) throw fail();
    const validated: RawActivityHour = {hour, active_ms:total, categories:categories.map(item=>({category:item.category,active_ms:item.duration}))};
    return {hour:validated.hour,duration:validated.active_ms,categories};
  });
  if (expected.size !== actual.size || [...expected].some(([category,duration])=>actual.get(category)!==duration)) throw fail();
  return hours;
}

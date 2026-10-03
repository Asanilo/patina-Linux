import { invoke } from "@tauri-apps/api/core";
import { getUiTextLanguage } from "../../shared/copy/uiText.ts";
import type { AppCategory } from "../../shared/classification/categoryTokens.ts";
import { getDailyApps, type DailyAppsRead } from "./dailyAppsRepository.ts";
export interface DashboardProductRead extends Omit<DailyAppsRead, "days"> {
  current: DailyAppsRead["days"][number];
  previous: DailyAppsRead["days"][number];
  hours: Array<{
    hour: number;
    duration: number;
    categories: Array<{
      category: AppCategory;
      duration: number;
    }>;
  }>;
}
function record(value: unknown): value is Record<string, unknown> {
  return value !== null && typeof value === "object" && !Array.isArray(value);
}
export async function getDashboardProduct(date: Date, request: (date: string) => Promise<unknown> = date => invoke("cmd_get_dashboard_product", { date, language: getUiTextLanguage() })): Promise<DashboardProductRead> {
  const current = new Date(date);
  current.setHours(0, 0, 0, 0);
  const previous = new Date(current);
  previous.setDate(previous.getDate() - 1);
  const end = new Date(current);
  end.setDate(end.getDate() + 1);
  const dateKey = `${current.getFullYear()}-${String(current.getMonth() + 1).padStart(2, "0")}-${String(current.getDate()).padStart(2, "0")}`;
  let hours: unknown;
  // Reuse the existing product validation and shared daily-read queue exactly once.
  const product = await getDailyApps(previous.getTime(), end.getTime(), async () => {
    const raw = await request(dateKey);
    if (!record(raw) || !record(raw.current) || !record(raw.previous))
      throw new Error("Invalid Dashboard snapshot");
    if (new TextEncoder().encode(JSON.stringify(raw)).length > 4 * 1024 * 1024)
      throw new Error("Dashboard snapshot exceeds budget");
    hours = raw.hours;
    return { ...raw, days: [raw.previous, raw.current] };
  });
  if (!Array.isArray(hours) || hours.length !== 24)
    throw new Error("Invalid Dashboard hours");
  const identities = new Map(product.applications.map(app => [app.appKey, app]));
  const expected = new Map<AppCategory, number>();
  for (const app of product.days[1].apps) {
    const category = identities.get(app.appKey)!.category;
    expected.set(category, (expected.get(category) ?? 0) + app.duration);
  }
  const actual = new Map<AppCategory, number>();
  const parsed = hours.map((hour: unknown, index: number) => {
    if (!record(hour) || hour.hour !== index || !Array.isArray(hour.categories) || hour.categories.length > 4096)
      throw new Error("Invalid Dashboard hour");
    let total = 0;
    const seen = new Set<string>();
    const categories = hour.categories.map((entry: unknown) => {
      if (!record(entry) || typeof entry.category !== "string" || !expected.has(entry.category as AppCategory) || seen.has(entry.category)
        || typeof entry.active_ms !== "number" || !Number.isSafeInteger(entry.active_ms) || entry.active_ms <= 0)
        throw new Error("Invalid Dashboard category quantity");
      seen.add(entry.category);
      const category = entry.category as AppCategory;
      total += entry.active_ms;
      actual.set(category, (actual.get(category) ?? 0) + entry.active_ms);
      if (!Number.isSafeInteger(total) || !Number.isSafeInteger(actual.get(category)))
        throw new Error("Dashboard quantity overflow");
      return { category, duration: entry.active_ms };
    });
    if (total !== hour.active_ms)
      throw new Error("Dashboard hour total mismatch");
    return { hour: index, duration: total, categories };
  });
  if (expected.size !== actual.size || [...expected].some(([category, duration]) => actual.get(category) !== duration))
    throw new Error("Dashboard category totals mismatch");
  return { sampledAtMs: product.sampledAtMs, configurationRevision: product.configurationRevision, trackingHealth: product.trackingHealth,
    applications: product.applications, current: product.days[1], previous: product.days[0], hours: parsed };
}

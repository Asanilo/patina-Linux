import { parseActivityHours, type ActivityHourRead } from "./hourlyActivitySnapshot.ts";
import { invoke } from "@tauri-apps/api/core";
import { getUiTextLanguage } from "../../shared/copy/uiText.ts";
import type { AppCategory } from "../../shared/classification/categoryTokens.ts";
import { getDailyApps, type DailyAppsRead } from "./dailyAppsRepository.ts";
export interface DashboardProductRead extends Omit<DailyAppsRead, "days"> {
  current: DailyAppsRead["days"][number];
  previous: DailyAppsRead["days"][number];
  hours: ActivityHourRead[];
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
  const identities = new Map(product.applications.map(app => [app.appKey, app]));
  const expected = new Map<AppCategory, number>();
  for (const app of product.days[1].apps) {
    const category = identities.get(app.appKey)!.category;
    expected.set(category, (expected.get(category) ?? 0) + app.duration);
  }
  const parsed = parseActivityHours(hours, expected);
  return { sampledAtMs: product.sampledAtMs, configurationRevision: product.configurationRevision, trackingHealth: product.trackingHealth,
    applications: product.applications, current: product.days[1], previous: product.days[0], hours: parsed };
}

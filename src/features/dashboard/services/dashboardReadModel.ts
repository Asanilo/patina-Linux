import { getIconMap } from "../../../platform/persistence/sessionReadRepository.ts";
import type { DashboardProductRead } from "../../../platform/persistence/dashboardRepository.ts";
import { AppClassification } from "../../../shared/classification/appClassification.ts";
import type { AppCategory } from "../../../shared/classification/categoryTokens.ts";
import { buildHourlyCategoryPresentation, type HourlyActivityPoint, type HourlyCategoryActivity } from "../../../shared/lib/hourlyActivityCompiler.ts";
import type { CategoryDistItem, TopApplicationItem } from "./dashboardFormatting.ts";
export interface DashboardSnapshot {
  fetchedAtMs: number;
  icons: Record<string, string>;
  product: DashboardProductRead;
}
export interface DashboardReadModel {
  totalTrackedTime: number;
  yesterdayTrackedTime: number;
  dayDeltaTrackedTime: number;
  topApplications: TopApplicationItem[];
  hourlyActivity: HourlyActivityPoint[];
  hourlyCategoryActivity: HourlyCategoryActivity;
  categoryDist: CategoryDistItem[];
  trackingHealth: DashboardProductRead["trackingHealth"] | null;
}
export async function loadDashboardSnapshot(date: Date = new Date()): Promise<DashboardSnapshot> {
  const { getDashboardProduct } = await import("../../../platform/persistence/dashboardRepository.ts");
  const [product, icons] = await Promise.all([
    getDashboardProduct(date),
    // Icons remain an explicit presentation-cache exception, not an activity fallback.
    getIconMap().catch((error) => { console.warn("Dashboard icons unavailable", error); return {}; }),
  ]);
  return { fetchedAtMs: product.sampledAtMs, product, icons };
}
export function buildDashboardReadModel(product: DashboardProductRead | null): DashboardReadModel {
  const identities = new Map(product?.applications.map(app => [app.appKey, app]) ?? []);
  const totalTrackedTime = product?.current.duration ?? 0;
  const yesterdayTrackedTime = product?.previous.duration ?? 0;
  const categoryTotals = new Map<AppCategory, number>();
  const topApplications = (product?.current.apps ?? []).map(item => {
    const identity = identities.get(item.appKey)!;
    const name = identity.displayNameOverride || AppClassification.resolveCanonicalDisplayName(item.appKey)
      || identity.appName || AppClassification.mapDefaultApp(item.appKey).name;
    categoryTotals.set(identity.category, (categoryTotals.get(identity.category) ?? 0) + item.duration);
    return { exeName: item.appKey, name, duration: item.duration,
      color: AppClassification.getCategoryColor(identity.category),
      percentage: totalTrackedTime > 0 ? Math.round(item.duration / totalTrackedTime * 100) : 0,
      categoryInitial: identity.category[0].toUpperCase() };
  }).sort((a, b) => b.duration - a.duration);
  const categoryDist = [...categoryTotals].map(([category, value]) => ({ category, value,
    name: AppClassification.getCategoryLabel(category), color: AppClassification.getCategoryColor(category),
  })).sort((a, b) => b.value - a.value);
  const hours = product?.hours ?? Array.from({ length: 24 }, (_, hour) => ({ hour, duration: 0, categories: [] }));
  return {
    totalTrackedTime, yesterdayTrackedTime, dayDeltaTrackedTime: totalTrackedTime - yesterdayTrackedTime,
    topApplications, categoryDist, trackingHealth: product?.trackingHealth ?? null,
    hourlyActivity: hours.map(hour => ({ hour: `${String(hour.hour).padStart(2, "0")}:00`, minutes: hour.duration < 60000 ? 0 : Math.round(hour.duration / 60000) })),
    hourlyCategoryActivity: buildHourlyCategoryPresentation(hours.map(hour => new Map(hour.categories.map(category => [category.category, category.duration / 60000])))),
  };
}

import { AppClassification } from "../classification/appClassification.ts";
import type { AppCategory } from "../classification/categoryTokens.ts";
import { UI_TEXT } from "../copy/uiText.ts";
import type { ActivityHour } from "../types/activityHours.ts";

export interface HourlyActivityPoint {
  hour: string;
  minutes: number;
}

export interface HourlyCategorySeries {
  dataKey: string;
  category: AppCategory | null;
  name: string;
  color: string;
  totalMinutes: number;
  isRemainder: boolean;
}

export interface HourlyCategoryActivityPoint {
  hour: string;
  minutes: number;
  segmentDetails: Record<string, HourlyCategoryActivitySegment>;
  [dataKey: string]: string | number | null | Record<string, HourlyCategoryActivitySegment>;
}

export interface HourlyCategoryActivitySegment {
  category: AppCategory | null;
  name: string;
  color: string;
  minutes: number;
  isRemainder: boolean;
}

export interface HourlyCategoryActivity {
  points: HourlyCategoryActivityPoint[];
  series: HourlyCategorySeries[];
}

interface CategoryDescriptor {
  category: AppCategory;
  name: string;
  color: string;
}

const HOURLY_CATEGORY_SLOT_PREFIX = "slot";

export function getHourlyCategorySlotDataKey(index: number) {
  return `${HOURLY_CATEGORY_SLOT_PREFIX}${index}`;
}

function formatHourlyDisplayMinutes(minutes: number) {
  if (!Number.isFinite(minutes) || minutes < 1) {
    return 0;
  }

  return Math.round(minutes);
}

function incrementCategoryMinutes(
  bucket: Map<AppCategory, number>,
  category: AppCategory,
  minutes: number,
) {
  bucket.set(category, (bucket.get(category) ?? 0) + minutes);
}

function buildVisibleSeries(
  categoryTotals: Map<AppCategory, number>,
  categoryDescriptors: Map<AppCategory, CategoryDescriptor>,
): HourlyCategorySeries[] {
  const sortedCategories = Array.from(categoryTotals.entries())
    .filter(([, totalMinutes]) => totalMinutes > 0)
    .sort((left, right) => right[1] - left[1]);
  return sortedCategories.map(([category, totalMinutes], index) => {
    const descriptor = categoryDescriptors.get(category);
    return {
      dataKey: `category${index}`,
      category,
      name: descriptor?.name ?? AppClassification.getCategoryLabel(category),
      color: descriptor?.color ?? AppClassification.getCategoryColor(category),
      totalMinutes,
      isRemainder: false,
    };
  });
}

export function buildHourlyCategoryPresentation(
  hourlyCategoryMinutes: readonly ReadonlyMap<AppCategory, number>[],
  categoryDescriptors: Map<AppCategory, CategoryDescriptor> = new Map(),
  existingTotals?: ReadonlyMap<AppCategory, number>,
): HourlyCategoryActivity {
  const categoryTotals = new Map<AppCategory, number>(existingTotals);
  for (const hour of existingTotals ? [] : hourlyCategoryMinutes) {
    for (const [category, minutes] of hour) incrementCategoryMinutes(categoryTotals, category, minutes);
  }
  const series = buildVisibleSeries(
    categoryTotals,
    categoryDescriptors,
  );
  const points = hourlyCategoryMinutes.map((categoryMinutes, hourIndex) => {
    const point: HourlyCategoryActivityPoint = {
      hour: `${hourIndex.toString().padStart(2, "0")}:00`,
      minutes: Array.from(categoryMinutes.values()).reduce((total, minutes) => total + minutes, 0),
      segmentDetails: {},
    };
    const activeSegments = series
      .map((item) => ({
        item,
        minutes: categoryMinutes.get(item.category as AppCategory) ?? 0,
      }))
      .filter(({ minutes }) => minutes > 0)
      .sort((left, right) => left.minutes - right.minutes);
    point.minutes = formatHourlyDisplayMinutes(point.minutes);
    let remainingMinutes = point.minutes;
    let visibleSlotIndex = 0;

    for (const [index, { item, minutes }] of activeSegments.entries()) {
      const roundedValue = index === activeSegments.length - 1
        ? remainingMinutes
        : Math.min(remainingMinutes, Math.round(minutes));
      if (roundedValue <= 0) {
        continue;
      }
      const slotDataKey = getHourlyCategorySlotDataKey(visibleSlotIndex);
      visibleSlotIndex += 1;
      point[slotDataKey] = roundedValue;
      point.segmentDetails[slotDataKey] = {
        category: item.category,
        name: item.name,
        color: item.color,
        minutes: roundedValue,
        isRemainder: item.isRemainder,
      };
      remainingMinutes -= roundedValue;
    }

    return point;
  });

  return { points, series };
}

export function limitHourlyCategoryActivity(
  activity: HourlyCategoryActivity,
  visibleCategoryLimit: number,
): HourlyCategoryActivity {
  const limit = Math.max(0, visibleCategoryLimit);
  const points = activity.points.map((sourcePoint) => {
    const activeSegments = Object.values(sourcePoint.segmentDetails)
      .filter((segment) => segment.minutes > 0)
      .sort((left, right) => right.minutes - left.minutes);
    const visibleSegments = activeSegments.slice(0, limit);
    const foldedSegments = activeSegments.slice(limit);
    const remainderMinutes = foldedSegments.reduce((total, segment) => total + segment.minutes, 0);
    const displaySegments = [
      ...(remainderMinutes > 0 ? [{
        category: null,
        name: UI_TEXT.hourlyActivityChart.remainingCategories,
        color: "var(--qp-text-tertiary)",
        minutes: remainderMinutes,
        isRemainder: true,
      } satisfies HourlyCategoryActivitySegment] : []),
      ...visibleSegments.sort((left, right) => left.minutes - right.minutes),
    ];
    const point: HourlyCategoryActivityPoint = {
      hour: sourcePoint.hour,
      minutes: sourcePoint.minutes,
      segmentDetails: {},
    };

    for (const [index, segment] of displaySegments.entries()) {
      const slotDataKey = getHourlyCategorySlotDataKey(index);
      point[slotDataKey] = segment.minutes;
      point.segmentDetails[slotDataKey] = segment;
    }

    return point;
  });

  return { points, series: activity.series };
}

/** Pure display formatting for confirmed backend quantities. Null is the empty loading view. */
export function buildHourlyProjectionPresentation(hours: readonly ActivityHour[] | null) {
  const confirmed = hours ?? Array.from({length:24},(_,hour)=>({hour,duration:0,categories:[]}));
  return {
    hourlyActivity: confirmed.map(hour=>({hour:`${String(hour.hour).padStart(2,"0")}:00`,minutes:formatHourlyDisplayMinutes(hour.duration/60000)})),
    hourlyCategoryActivity: buildHourlyCategoryPresentation(confirmed.map(hour=>new Map(hour.categories.map(category=>[category.category,category.duration/60000])))),
  };
}

/** A repeated civil hour (or retained native overlap) can exceed 60 minutes. */
export function getHourlyActivityAxisMaximum(points: ReadonlyArray<{minutes: number}>): number {
  const maximum = points.reduce((max, point) => Number.isFinite(point.minutes) ? Math.max(max, point.minutes) : max, 60);
  return Math.ceil(maximum / 60) * 60;
}

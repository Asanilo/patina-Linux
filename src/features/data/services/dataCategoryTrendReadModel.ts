import { AppClassification } from "../../../shared/classification/appClassification.ts";
import { isAppCategory, type AppCategory } from "../../../shared/classification/categoryTokens.ts";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import {
  buildDataChartAxis,
  buildDataTrendSessionContext,
  getClippedSessionDuration,
  type AggregateSessionRecord,
  type CompiledDataSession,
  type DataAppDayRow,
  type DataAppTrendViewModel,
  type DataTrendPoint,
} from "./dataReadModel.ts";
import type { ResolvedDataTrendRange } from "./dataTrendRange.ts";

export interface DataCategoryOption {
  category: AppCategory;
  displayName: string;
  color: string;
  appCount: number;
  totalDuration: number;
  percentage: number;
  averageDuration: number;
  activeDayCount: number;
}

export interface DataCategoryTrendSeries {
  key: AppCategory;
  dataKey: string;
  displayName: string;
  color: string;
}

export interface DataCategoryTrendChartRow {
  label: string;
  date: string;
  duration: number;
  hours: number;
  totalDuration: number;
  totalHours: number;
  [key: string]: string | number;
}

export interface DataCategoryTrendViewModel {
  range: ResolvedDataTrendRange;
  rangeLabel: string;
  granularity: "day" | "month";
  categoryOptions: DataCategoryOption[];
  selectedCategories: DataCategoryOption[];
  chartSeries: DataCategoryTrendSeries[];
  chartRows: DataCategoryTrendChartRow[];
  summary: {
    totalDuration: number;
    averageDuration: number;
    activeDayCount: number;
  };
  chartAxis: DataAppTrendViewModel["chartAxis"];
  peakDay: DataAppDayRow | null;
}

interface CategoryBucket {
  category: AppCategory;
  sessions: CompiledDataSession[];
  appKeys: Set<string>;
  totalDuration: number;
}

function toDateKey(timestamp: number) {
  const date = new Date(timestamp);
  return [
    date.getFullYear(),
    String(date.getMonth() + 1).padStart(2, "0"),
    String(date.getDate()).padStart(2, "0"),
  ].join("-");
}

function formatDayLabel(dateKey: string) {
  return new Date(`${dateKey}T00:00:00`).toLocaleDateString(undefined, {
    month: "2-digit",
    day: "2-digit",
    weekday: "short",
  });
}

function getCategory(session: CompiledDataSession) {
  return AppClassification.mapApp(session.appKey, { appName: session.displayName }).category;
}

function sumDurationInRange(
  sessions: readonly CompiledDataSession[],
  startMs: number,
  endMs: number,
) {
  return sessions.reduce(
    (sum, session) => sum + getClippedSessionDuration(session, startMs, endMs),
    0,
  );
}

function buildCategoryBuckets(sessions: readonly CompiledDataSession[]) {
  const buckets = new Map<AppCategory, CategoryBucket>();

  for (const session of sessions) {
    const category = getCategory(session);
    const bucket = buckets.get(category) ?? {
      category,
      sessions: [],
      appKeys: new Set<string>(),
      totalDuration: 0,
    };
    bucket.sessions.push(session);
    bucket.appKeys.add(session.appKey);
    bucket.totalDuration += Math.max(0, session.endTime - session.startTime);
    buckets.set(category, bucket);
  }

  return Array.from(buckets.values()).sort((left, right) => (
    right.totalDuration - left.totalDuration
    || left.category.localeCompare(right.category)
  ));
}

function createCategoryOption(
  bucket: CategoryBucket,
  totalDuration: number,
  averageDivisor: number,
  activeDayCount: number,
): DataCategoryOption {
  return {
    category: bucket.category,
    displayName: AppClassification.getCategoryLabel(bucket.category),
    color: AppClassification.getCategoryColor(bucket.category),
    appCount: bucket.appKeys.size,
    totalDuration: bucket.totalDuration,
    percentage: totalDuration > 0 ? (bucket.totalDuration / totalDuration) * 100 : 0,
    averageDuration: Math.round(bucket.totalDuration / averageDivisor),
    activeDayCount,
  };
}

function createEmptyCategoryOption(category: AppCategory): DataCategoryOption {
  return {
    category,
    displayName: AppClassification.getCategoryLabel(category),
    color: AppClassification.getCategoryColor(category),
    appCount: 0,
    totalDuration: 0,
    percentage: 0,
    averageDuration: 0,
    activeDayCount: 0,
  };
}

export function buildDataCategoryTrendViewModel(
  sessions: AggregateSessionRecord[],
  range: ResolvedDataTrendRange,
  nowMs: number,
  selectedCategoryKeys: readonly string[],
): DataCategoryTrendViewModel {
  const context = buildDataTrendSessionContext(
    sessions,
    range,
    nowMs,
  );
  const chartRanges = context.range.granularity === "month"
    ? context.monthRanges
    : context.dayRanges;
  const averageDivisor = Math.max(1, chartRanges.length);
  const buckets = buildCategoryBuckets(context.sessions);
  const bucketByCategory = new Map(buckets.map((bucket) => [bucket.category, bucket]));
  const totalDuration = buckets.reduce((sum, bucket) => sum + bucket.totalDuration, 0);
  const options = buckets.map((bucket) => createCategoryOption(
    bucket,
    totalDuration,
    averageDivisor,
    context.dayRanges.filter((range) => (
      sumDurationInRange(bucket.sessions, range.startMs, range.endMs) > 0
    )).length,
  ));
  const requestedCategories = Array.from(new Set(selectedCategoryKeys)).filter(isAppCategory);
  const selectedCategoryIds = requestedCategories.length > 0
    ? requestedCategories
    : options[0] ? [options[0].category] : [];
  const selectedCategories = selectedCategoryIds.map((category) => (
    options.find((option) => option.category === category)
      ?? createEmptyCategoryOption(category)
  ));
  const selectedBuckets = selectedCategoryIds.flatMap((category) => {
    const bucket = bucketByCategory.get(category);
    return bucket ? [bucket] : [];
  });
  const chartSeries = selectedCategories.map((category, index) => ({
    key: category.category,
    dataKey: `series${index}`,
    displayName: category.displayName,
    color: category.color,
  }));
  const chartRows = chartRanges.map((range) => {
    const date = toDateKey(range.startMs);
    const row: DataCategoryTrendChartRow = {
      label: context.range.granularity === "month"
        ? UI_TEXT.date.monthLabel(Number(date.slice(5, 7)))
        : date.slice(5),
      date,
      duration: 0,
      hours: 0,
      totalDuration: 0,
      totalHours: 0,
    };

    selectedBuckets.forEach((bucket, index) => {
      const duration = sumDurationInRange(bucket.sessions, range.startMs, range.endMs);
      row[`series${index}`] = duration / 3_600_000;
      row.totalDuration += duration;
    });
    row.totalHours = row.totalDuration / 3_600_000;
    row.duration = row.totalDuration;
    row.hours = row.totalHours;
    return row;
  });
  const dayRows = context.dayRanges.map((range) => {
    const date = toDateKey(range.startMs);
    return {
      date,
      label: formatDayLabel(date),
      duration: selectedBuckets.reduce(
        (sum, bucket) => sum + sumDurationInRange(bucket.sessions, range.startMs, range.endMs),
        0,
      ),
      intensity: 0,
    };
  });
  const peakDay = dayRows.reduce<DataAppDayRow | null>((peak, row) => (
    !peak || row.duration > peak.duration ? row : peak
  ), null);
  const selectedTotalDuration = selectedCategories.reduce(
    (sum, category) => sum + category.totalDuration,
    0,
  );
  const axisPoints: DataTrendPoint[] = chartRows.map((row) => ({
    label: row.label,
    date: row.date,
    hours: row.totalHours,
  }));

  return {
    range: context.range,
    rangeLabel: context.range.label,
    granularity: context.range.granularity,
    categoryOptions: options,
    selectedCategories,
    chartSeries,
    chartRows,
    summary: {
      totalDuration: selectedTotalDuration,
      averageDuration: Math.round(selectedTotalDuration / averageDivisor),
      activeDayCount: dayRows.filter((row) => row.duration > 0).length,
    },
    chartAxis: buildDataChartAxis(axisPoints),
    peakDay: peakDay && peakDay.duration > 0 ? peakDay : null,
  };
}

export function filterDataCategoryOptionsForQuery(
  options: readonly DataCategoryOption[],
  query: string,
) {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) return [...options];
  return options.filter((option) => (
    option.displayName.toLocaleLowerCase().includes(normalizedQuery)
  ));
}

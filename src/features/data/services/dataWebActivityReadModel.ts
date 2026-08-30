import { AppClassification } from "../../../shared/classification/appClassification.ts";
import type { AppCategory } from "../../../shared/classification/categoryTokens.ts";
import { resolveStableDomainColor } from "../../../shared/classification/domainColor.ts";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import type {
  WebActivityTrendSegment,
  WebDomainOverride,
} from "../../../shared/types/webActivity.ts";
import {
  buildDataChartAxis,
  type DataAppDayRow,
  type DataAppTrendViewModel,
  type DataDestinationTrendChartRow,
  type DataDestinationTrendSeries,
  type DataTrendPoint,
} from "./dataReadModel.ts";
import {
  buildDataDayRanges,
  buildDataMonthRanges,
  type ResolvedDataTrendRange,
} from "./dataTrendRange.ts";

export interface DataWebDomainOption {
  normalizedDomain: string;
  displayName: string;
  category: AppCategory;
  faviconUrl: string | null;
  color: string;
  totalDuration: number;
  percentage: number;
  averageDuration: number;
  activeDayCount: number;
}

export interface DataWebActivityTrendViewModel {
  range: ResolvedDataTrendRange;
  rangeLabel: string;
  granularity: "day" | "month";
  domainOptions: DataWebDomainOption[];
  selectedDomains: DataWebDomainOption[];
  chartSeries: DataDestinationTrendSeries[];
  chartRows: DataDestinationTrendChartRow[];
  summary: {
    totalDuration: number;
    averageDuration: number;
    activeDayCount: number;
  };
  chartAxis: DataAppTrendViewModel["chartAxis"];
  peakDay: DataAppDayRow | null;
}

interface CompiledWebTrendSegment {
  normalizedDomain: string;
  domain: string;
  faviconUrl: string | null;
  startTime: number;
  endTime: number;
}

interface WebDomainBucket {
  normalizedDomain: string;
  domain: string;
  faviconUrl: string | null;
  segments: CompiledWebTrendSegment[];
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

function preferFaviconUrl(current: string | null, candidate: string | null) {
  if (!candidate) return current;
  if (!current || (!current.startsWith("data:") && candidate.startsWith("data:"))) {
    return candidate;
  }
  return current;
}

function compileSegments(
  segments: readonly WebActivityTrendSegment[],
  range: ResolvedDataTrendRange,
  nowMs: number,
) {
  return segments.flatMap((segment): CompiledWebTrendSegment[] => {
    const normalizedDomain = segment.normalizedDomain.trim().toLocaleLowerCase();
    if (!normalizedDomain) return [];
    const startTime = Math.max(range.startMs, segment.startTime);
    const rawEndTime = segment.endTime ?? nowMs;
    const endTime = Math.min(range.endMs, Math.max(segment.startTime, rawEndTime));
    if (endTime <= startTime) return [];
    return [{
      normalizedDomain,
      domain: segment.domain.trim() || normalizedDomain,
      faviconUrl: segment.faviconUrl,
      startTime,
      endTime,
    }];
  });
}

function sumDurationInRange(
  segments: readonly CompiledWebTrendSegment[],
  startMs: number,
  endMs: number,
) {
  return segments.reduce((sum, segment) => (
    sum + Math.max(0, Math.min(segment.endTime, endMs) - Math.max(segment.startTime, startMs))
  ), 0);
}

function buildDomainBuckets(
  segments: readonly CompiledWebTrendSegment[],
  overrides: Record<string, WebDomainOverride>,
) {
  const buckets = new Map<string, WebDomainBucket>();

  for (const segment of segments) {
    if (overrides[segment.normalizedDomain]?.enabled === false) continue;
    const bucket = buckets.get(segment.normalizedDomain) ?? {
      normalizedDomain: segment.normalizedDomain,
      domain: segment.domain,
      faviconUrl: null,
      segments: [],
      totalDuration: 0,
    };
    bucket.domain = segment.domain || bucket.domain;
    bucket.faviconUrl = preferFaviconUrl(bucket.faviconUrl, segment.faviconUrl);
    bucket.segments.push(segment);
    bucket.totalDuration += segment.endTime - segment.startTime;
    buckets.set(segment.normalizedDomain, bucket);
  }

  return Array.from(buckets.values());
}

function resolveDomainColor(
  normalizedDomain: string,
  category: AppCategory,
  override: WebDomainOverride | undefined,
) {
  if (override?.color) return override.color;
  if (category !== "other") return AppClassification.getCategoryColor(category);
  return resolveStableDomainColor(normalizedDomain);
}

function createDomainOption(
  bucket: WebDomainBucket,
  overrides: Record<string, WebDomainOverride>,
  totalDuration: number,
  averageDivisor: number,
  activeDayCount: number,
): DataWebDomainOption {
  const override = overrides[bucket.normalizedDomain];
  const category = override?.category ?? "other";
  return {
    normalizedDomain: bucket.normalizedDomain,
    displayName: override?.displayName?.trim() || bucket.domain || bucket.normalizedDomain,
    category,
    faviconUrl: bucket.faviconUrl,
    color: resolveDomainColor(bucket.normalizedDomain, category, override),
    totalDuration: bucket.totalDuration,
    percentage: totalDuration > 0 ? (bucket.totalDuration / totalDuration) * 100 : 0,
    averageDuration: Math.round(bucket.totalDuration / averageDivisor),
    activeDayCount,
  };
}

function createEmptyDomainOption(
  normalizedDomain: string,
  overrides: Record<string, WebDomainOverride>,
) {
  const override = overrides[normalizedDomain];
  const category = override?.category ?? "other";
  return {
    normalizedDomain,
    displayName: override?.displayName?.trim() || normalizedDomain,
    category,
    faviconUrl: null,
    color: resolveDomainColor(normalizedDomain, category, override),
    totalDuration: 0,
    percentage: 0,
    averageDuration: 0,
    activeDayCount: 0,
  } satisfies DataWebDomainOption;
}

export function buildDataWebActivityTrendViewModel(
  segments: readonly WebActivityTrendSegment[],
  overrides: Record<string, WebDomainOverride>,
  range: ResolvedDataTrendRange,
  nowMs: number,
  selectedDomainKeys: readonly string[],
): DataWebActivityTrendViewModel {
  const dayRanges = buildDataDayRanges(range);
  const chartRanges = range.granularity === "month"
    ? buildDataMonthRanges(range)
    : dayRanges;
  const averageDivisor = Math.max(1, chartRanges.length);
  const buckets = buildDomainBuckets(compileSegments(segments, range, nowMs), overrides)
    .sort((left, right) => (
      right.totalDuration - left.totalDuration
      || left.normalizedDomain.localeCompare(right.normalizedDomain)
    ));
  const bucketByDomain = new Map(buckets.map((bucket) => [bucket.normalizedDomain, bucket]));
  const totalDuration = buckets.reduce((sum, bucket) => sum + bucket.totalDuration, 0);
  const options = buckets.map((bucket) => createDomainOption(
    bucket,
    overrides,
    totalDuration,
    averageDivisor,
    dayRanges.filter((dayRange) => (
      sumDurationInRange(bucket.segments, dayRange.startMs, dayRange.endMs) > 0
    )).length,
  ));
  const requestedDomains = Array.from(new Set(
    selectedDomainKeys.map((domain) => domain.trim().toLocaleLowerCase()).filter(Boolean),
  ));
  const selectedDomainIds = requestedDomains.length > 0
    ? requestedDomains
    : options[0] ? [options[0].normalizedDomain] : [];
  const selectedDomains = selectedDomainIds.map((domain) => (
    options.find((option) => option.normalizedDomain === domain)
      ?? createEmptyDomainOption(domain, overrides)
  ));
  const selectedBuckets = selectedDomainIds.flatMap((domain) => {
    const bucket = bucketByDomain.get(domain);
    return bucket ? [bucket] : [];
  });
  const chartSeries = selectedDomains.map((domain, index) => ({
    key: domain.normalizedDomain,
    dataKey: `series${index}`,
    displayName: domain.displayName,
    color: domain.color,
  }));
  const chartRows = chartRanges.map((chartRange) => {
    const date = toDateKey(chartRange.startMs);
    const row: DataDestinationTrendChartRow = {
      label: range.granularity === "month"
        ? UI_TEXT.date.monthLabel(Number(date.slice(5, 7)))
        : date.slice(5),
      date,
      duration: 0,
      hours: 0,
      totalDuration: 0,
      totalHours: 0,
    };
    selectedBuckets.forEach((bucket, index) => {
      const duration = sumDurationInRange(
        bucket.segments,
        chartRange.startMs,
        chartRange.endMs,
      );
      row[`series${index}`] = duration / 3_600_000;
      row.totalDuration += duration;
    });
    row.duration = row.totalDuration;
    row.hours = row.totalDuration / 3_600_000;
    row.totalHours = row.hours;
    return row;
  });
  const dayRows = dayRanges.map((dayRange) => {
    const date = toDateKey(dayRange.startMs);
    return {
      date,
      label: formatDayLabel(date),
      duration: selectedBuckets.reduce((sum, bucket) => (
        sum + sumDurationInRange(bucket.segments, dayRange.startMs, dayRange.endMs)
      ), 0),
      intensity: 0,
    };
  });
  const peakDay = dayRows.reduce<DataAppDayRow | null>((peak, row) => (
    !peak || row.duration > peak.duration ? row : peak
  ), null);
  const selectedTotalDuration = selectedDomains.reduce(
    (sum, domain) => sum + domain.totalDuration,
    0,
  );
  const axisPoints: DataTrendPoint[] = chartRows.map((row) => ({
    label: row.label,
    date: row.date,
    hours: row.totalHours,
  }));

  return {
    range,
    rangeLabel: range.label,
    granularity: range.granularity,
    domainOptions: options,
    selectedDomains,
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

export function filterDataWebDomainOptionsForQuery(
  options: readonly DataWebDomainOption[],
  query: string,
) {
  const normalizedQuery = query.trim().toLocaleLowerCase();
  if (!normalizedQuery) return [...options];
  return options.filter((option) => (
    option.displayName.toLocaleLowerCase().includes(normalizedQuery)
    || option.normalizedDomain.includes(normalizedQuery)
  ));
}

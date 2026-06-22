import { AppClassification } from "../../../shared/classification/appClassification.ts";
import type { AppCategory } from "../../../shared/classification/categoryTokens.ts";
import type { CompiledSession } from "../../../shared/lib/sessionReadCompiler.ts";

const MINUTE_MS = 60 * 1000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;
const HALF_HOUR_MS = 30 * MINUTE_MS;
const MINUTE_BOUNDARY_SNAP_MS = 1_000;
const MIN_VISIBLE_TIMELINE_SEGMENT_MS = 30_000;

export type HistoryTimelineDisplayMode = "app" | "category";
export const HISTORY_TIMELINE_ZOOM_OPTIONS = [24, 12, 8, 4, 1] as const;
export type HistoryTimelineZoomHours = typeof HISTORY_TIMELINE_ZOOM_OPTIONS[number];
export const DEFAULT_HISTORY_TIMELINE_ZOOM_HOURS: HistoryTimelineZoomHours = 24;

export interface HistoryTimelineViewport {
  startMs: number;
  endMs: number;
  zoomHours: HistoryTimelineZoomHours;
}

export interface HistoryTimelineAxisTick {
  label: string;
  ratio: number;
}

export interface HistoryTimelineSegment {
  id: string;
  sourceSessionId: number;
  timelineKey: string;
  appKey: string;
  exeName: string;
  displayName: string;
  displayTitle: string;
  category: AppCategory;
  categoryLabel: string;
  startTime: number;
  endTime: number;
  duration: number;
  startRatio: number;
  endRatio: number;
  widthRatio: number;
  titleSamples: string[];
  titleSampleDetails: Array<{
    title: string;
    startTime: number;
    endTime: number;
  }>;
  alternateLabels: string[];
  isLive: boolean;
}

export interface HistoryTimelineLegendItem {
  key: string;
  label: string;
  duration: number;
  percentage: number;
  category: AppCategory;
  exeName: string;
}

export interface HistoryTimelineViewModel {
  segments: HistoryTimelineSegment[];
  legendItems: HistoryTimelineLegendItem[];
  axisTicks: HistoryTimelineAxisTick[];
  dayStartMs: number;
  dayEndMs: number;
  viewportStartMs: number;
  viewportEndMs: number;
  viewportDurationMs: number;
  zoomHours: HistoryTimelineZoomHours;
  visibleEndMs: number;
  visibleEndRatio: number;
}

interface BuildHistoryTimelineViewModelParams {
  sessions: CompiledSession[];
  selectedDate: Date;
  nowMs: number;
  mode: HistoryTimelineDisplayMode;
  mergeThresholdSecs?: number;
  viewport?: HistoryTimelineViewport;
}

function getFullDayRange(date: Date) {
  const dayStart = new Date(date);
  dayStart.setHours(0, 0, 0, 0);

  return {
    dayStartMs: dayStart.getTime(),
    dayEndMs: dayStart.getTime() + DAY_MS,
  };
}

function clampRatio(value: number) {
  if (!Number.isFinite(value)) return 0;
  return Math.min(1, Math.max(0, value));
}

function clampNumber(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) return min;
  return Math.min(max, Math.max(min, value));
}

export function getHistoryTimelineZoomDurationMs(zoomHours: HistoryTimelineZoomHours) {
  return Math.min(DAY_MS, Math.max(HOUR_MS, zoomHours * HOUR_MS));
}

export function normalizeHistoryTimelineViewport({
  selectedDate,
  zoomHours,
  requestedStartMs,
}: {
  selectedDate: Date;
  zoomHours: HistoryTimelineZoomHours;
  requestedStartMs?: number | null;
}): HistoryTimelineViewport {
  const { dayStartMs, dayEndMs } = getFullDayRange(selectedDate);
  const durationMs = getHistoryTimelineZoomDurationMs(zoomHours);

  if (zoomHours === 24 || durationMs >= DAY_MS) {
    return {
      startMs: dayStartMs,
      endMs: dayEndMs,
      zoomHours: 24,
    };
  }

  const maxStartMs = dayEndMs - durationMs;
  const safeRequestedStartMs = typeof requestedStartMs === "number" && Number.isFinite(requestedStartMs)
    ? requestedStartMs
    : dayStartMs;
  const startMs = clampNumber(safeRequestedStartMs, dayStartMs, maxStartMs);

  return {
    startMs,
    endMs: startMs + durationMs,
    zoomHours,
  };
}

export function snapHistoryTimelineFocusToNearestHalfHour({
  selectedDate,
  requestedTimeMs,
}: {
  selectedDate: Date;
  requestedTimeMs: number;
}) {
  const { dayStartMs, dayEndMs } = getFullDayRange(selectedDate);
  const safeRequestedTimeMs = typeof requestedTimeMs === "number" && Number.isFinite(requestedTimeMs)
    ? requestedTimeMs
    : dayStartMs;
  const snappedTimeMs = dayStartMs
    + Math.round((safeRequestedTimeMs - dayStartMs) / HALF_HOUR_MS) * HALF_HOUR_MS;

  return clampNumber(snappedTimeMs, dayStartMs, dayEndMs);
}

export function normalizeHistoryTimelineViewportAroundFocus({
  selectedDate,
  zoomHours,
  focusTimeMs,
}: {
  selectedDate: Date;
  zoomHours: HistoryTimelineZoomHours;
  focusTimeMs: number;
}) {
  const focusMs = snapHistoryTimelineFocusToNearestHalfHour({
    selectedDate,
    requestedTimeMs: focusTimeMs,
  });
  const durationMs = getHistoryTimelineZoomDurationMs(zoomHours);

  return normalizeHistoryTimelineViewport({
    selectedDate,
    zoomHours,
    requestedStartMs: focusMs - durationMs / 2,
  });
}

function formatAxisLabel(timeMs: number, dayEndMs: number) {
  if (timeMs === dayEndMs) {
    return "24:00";
  }

  const time = new Date(timeMs);
  return `${String(time.getHours()).padStart(2, "0")}:${String(time.getMinutes()).padStart(2, "0")}`;
}

function getAxisIntervalMs(zoomHours: HistoryTimelineZoomHours) {
  switch (zoomHours) {
    case 1:
      return 15 * MINUTE_MS;
    case 4:
      return HOUR_MS;
    case 8:
      return 2 * HOUR_MS;
    case 12:
      return 3 * HOUR_MS;
    case 24:
    default:
      return 6 * HOUR_MS;
  }
}

function buildAxisTicks(
  viewport: HistoryTimelineViewport,
  dayEndMs: number,
): HistoryTimelineAxisTick[] {
  const viewportDurationMs = Math.max(1, viewport.endMs - viewport.startMs);
  const intervalMs = getAxisIntervalMs(viewport.zoomHours);
  const ticks: HistoryTimelineAxisTick[] = [];

  for (let timeMs = viewport.startMs; timeMs < viewport.endMs; timeMs += intervalMs) {
    ticks.push({
      label: formatAxisLabel(timeMs, dayEndMs),
      ratio: clampRatio((timeMs - viewport.startMs) / viewportDurationMs),
    });
  }

  const lastTick = ticks[ticks.length - 1];
  if (!lastTick || lastTick.ratio < 1) {
    ticks.push({
      label: formatAxisLabel(viewport.endMs, dayEndMs),
      ratio: 1,
    });
  }

  return ticks;
}

function resolveVisibleEndMs(selectedDate: Date, nowMs: number, dayStartMs: number, dayEndMs: number) {
  const nowDate = new Date(nowMs);
  const selectedIsToday = selectedDate.toDateString() === nowDate.toDateString();
  if (!selectedIsToday) {
    return dayEndMs;
  }

  return Math.min(dayEndMs, Math.max(dayStartMs, nowMs));
}

function clipTitleSampleDetails(
  session: CompiledSession,
  clippedStart: number,
  clippedEnd: number,
) {
  const details = session.titleSampleDetails
    .map((sample) => ({
      title: sample.title,
      startTime: Math.max(sample.startTime, clippedStart),
      endTime: Math.min(sample.endTime, clippedEnd),
    }))
    .filter((sample) => sample.title.trim() && sample.endTime > sample.startTime);

  if (details.length > 0) {
    return details;
  }

  const fallbackTitle = session.displayTitle.trim();
  if (!fallbackTitle) {
    return [];
  }

  return [{
    title: fallbackTitle,
    startTime: clippedStart,
    endTime: clippedEnd,
  }];
}

function clipSegmentTitleSampleDetails(
  segment: HistoryTimelineSegment,
  clippedStart: number,
  clippedEnd: number,
) {
  return segment.titleSampleDetails
    .map((sample) => ({
      title: sample.title,
      startTime: Math.max(sample.startTime, clippedStart),
      endTime: Math.min(sample.endTime, clippedEnd),
    }))
    .filter((sample) => sample.title.trim() && sample.endTime > sample.startTime);
}

function buildSegment(
  session: CompiledSession,
  dayStartMs: number,
  dayEndMs: number,
  visibleEndMs: number,
  viewport: HistoryTimelineViewport,
): HistoryTimelineSegment | null {
  const rawEndTime = Math.max(session.startTime, session.endTime ?? session.startTime);
  const clippedStart = Math.max(session.startTime, dayStartMs, viewport.startMs);
  const clippedEnd = Math.min(rawEndTime, dayEndMs, visibleEndMs, viewport.endMs);

  if (clippedEnd <= clippedStart) {
    return null;
  }

  const mapped = AppClassification.mapApp(session.appKey, { appName: session.displayName });
  const viewportDurationMs = Math.max(1, viewport.endMs - viewport.startMs);
  const startRatio = clampRatio((clippedStart - viewport.startMs) / viewportDurationMs);
  const endRatio = clampRatio((clippedEnd - viewport.startMs) / viewportDurationMs);
  const titleSampleDetails = clipTitleSampleDetails(session, clippedStart, clippedEnd);

  return {
    id: `${session.id}-${clippedStart}-${clippedEnd}`,
    sourceSessionId: session.id,
    timelineKey: `app:${session.appKey}`,
    appKey: session.appKey,
    exeName: session.exeName,
    displayName: session.displayName,
    displayTitle: session.displayTitle,
    category: mapped.category,
    categoryLabel: AppClassification.getCategoryLabel(mapped.category),
    startTime: clippedStart,
    endTime: clippedEnd,
    duration: clippedEnd - clippedStart,
    startRatio,
    endRatio,
    widthRatio: Math.max(0, endRatio - startRatio),
    titleSamples: titleSampleDetails.map((sample) => sample.title),
    titleSampleDetails,
    alternateLabels: [],
    isLive: session.isLive,
  };
}

function mergeTitleSampleDetails(
  current: HistoryTimelineSegment["titleSampleDetails"],
  next: HistoryTimelineSegment["titleSampleDetails"],
) {
  const sorted = [...current, ...next]
    .filter((sample) => sample.title.trim() && sample.endTime > sample.startTime)
    .sort((a, b) => a.startTime - b.startTime || a.endTime - b.endTime);

  return sorted.reduce<HistoryTimelineSegment["titleSampleDetails"]>((merged, sample) => {
    const previous = merged[merged.length - 1];
    if (previous?.title === sample.title && sample.startTime <= previous.endTime) {
      previous.endTime = Math.max(previous.endTime, sample.endTime);
      return merged;
    }

    merged.push({ ...sample });
    return merged;
  }, []);
}

function mergeAlternateLabels(current: string[], next: string[]) {
  return Array.from(new Set([...current, ...next]));
}

function mergeAdjacentTimelineSegments(
  current: HistoryTimelineSegment,
  next: HistoryTimelineSegment,
): HistoryTimelineSegment {
  const endTime = Math.max(current.endTime, next.endTime);
  const startRatio = Math.min(current.startRatio, next.startRatio);
  const endRatio = Math.max(current.endRatio, next.endRatio);
  const titleSampleDetails = mergeTitleSampleDetails(
    current.titleSampleDetails,
    next.titleSampleDetails,
  );

  return {
    ...current,
    id: `${current.id}_${next.sourceSessionId}-${next.endTime}`,
    endTime,
    duration: current.duration + next.duration,
    startRatio,
    endRatio,
    widthRatio: Math.max(0, endRatio - startRatio),
    titleSamples: titleSampleDetails.map((sample) => sample.title),
    titleSampleDetails,
    alternateLabels: mergeAlternateLabels(current.alternateLabels, next.alternateLabels),
    isLive: current.isLive || next.isLive,
  };
}

function resolveTimelineKey(segment: HistoryTimelineSegment, mode: HistoryTimelineDisplayMode) {
  return mode === "category" ? `category:${segment.category}` : `app:${segment.appKey}`;
}

interface MinuteTimelineItem {
  key: string;
  appKey: string;
  exeName: string;
  displayName: string;
  displayTitle: string;
  category: AppCategory;
  categoryLabel: string;
  duration: number;
  firstSeenAt: number;
  sourceSessionIds: number[];
  titleSampleDetails: HistoryTimelineSegment["titleSampleDetails"];
  isLive: boolean;
}

interface MinuteTimelineBucket {
  startTime: number;
  endTime: number;
  activeStartTime: number | null;
  activeEndTime: number | null;
  items: Map<string, MinuteTimelineItem>;
}

function getMinuteStart(timeMs: number, dayStartMs: number) {
  return dayStartMs + Math.floor((timeMs - dayStartMs) / MINUTE_MS) * MINUTE_MS;
}

function getOrCreateMinuteBucket(
  buckets: Map<number, MinuteTimelineBucket>,
  minuteStart: number,
  visibleEndMs: number,
) {
  const existing = buckets.get(minuteStart);
  if (existing) {
    return existing;
  }

  const bucket = {
    startTime: minuteStart,
    endTime: Math.min(minuteStart + MINUTE_MS, visibleEndMs),
    activeStartTime: null,
    activeEndTime: null,
    items: new Map<string, MinuteTimelineItem>(),
  };
  buckets.set(minuteStart, bucket);
  return bucket;
}

function snapToMinuteBoundary(value: number, boundary: number) {
  return Math.abs(value - boundary) <= MINUTE_BOUNDARY_SNAP_MS ? boundary : value;
}

function addSegmentOverlapToMinuteBucket(
  bucket: MinuteTimelineBucket,
  segment: HistoryTimelineSegment,
  mode: HistoryTimelineDisplayMode,
  overlapStart: number,
  overlapEnd: number,
) {
  const key = resolveTimelineKey(segment, mode);
  const existing = bucket.items.get(key);
  const titleSampleDetails = clipSegmentTitleSampleDetails(segment, overlapStart, overlapEnd);
  const visibleOverlapStart = snapToMinuteBoundary(overlapStart, bucket.startTime);
  const visibleOverlapEnd = snapToMinuteBoundary(overlapEnd, bucket.endTime);

  bucket.activeStartTime = bucket.activeStartTime === null
    ? visibleOverlapStart
    : Math.min(bucket.activeStartTime, visibleOverlapStart);
  bucket.activeEndTime = bucket.activeEndTime === null
    ? visibleOverlapEnd
    : Math.max(bucket.activeEndTime, visibleOverlapEnd);

  if (existing) {
    existing.duration += overlapEnd - overlapStart;
    existing.firstSeenAt = Math.min(existing.firstSeenAt, overlapStart);
    existing.sourceSessionIds = Array.from(new Set([
      ...existing.sourceSessionIds,
      segment.sourceSessionId,
    ]));
    existing.titleSampleDetails = mergeTitleSampleDetails(
      existing.titleSampleDetails,
      titleSampleDetails,
    );
    existing.isLive = existing.isLive || segment.isLive;
    return;
  }

  bucket.items.set(key, {
    key,
    appKey: segment.appKey,
    exeName: segment.exeName,
    displayName: segment.displayName,
    displayTitle: segment.displayTitle,
    category: segment.category,
    categoryLabel: segment.categoryLabel,
    duration: overlapEnd - overlapStart,
    firstSeenAt: overlapStart,
    sourceSessionIds: [segment.sourceSessionId],
    titleSampleDetails,
    isLive: segment.isLive,
  });
}

function buildMinuteBuckets(
  segments: HistoryTimelineSegment[],
  dayStartMs: number,
  visibleEndMs: number,
  mode: HistoryTimelineDisplayMode,
) {
  const buckets = new Map<number, MinuteTimelineBucket>();

  for (const segment of segments) {
    let minuteStart = getMinuteStart(segment.startTime, dayStartMs);

    while (minuteStart < segment.endTime && minuteStart < visibleEndMs) {
      const bucket = getOrCreateMinuteBucket(buckets, minuteStart, visibleEndMs);
      const overlapStart = Math.max(segment.startTime, bucket.startTime);
      const overlapEnd = Math.min(segment.endTime, bucket.endTime);

      if (overlapEnd > overlapStart) {
        addSegmentOverlapToMinuteBucket(bucket, segment, mode, overlapStart, overlapEnd);
      }

      minuteStart += MINUTE_MS;
    }
  }

  return Array.from(buckets.values()).sort((a, b) => a.startTime - b.startTime);
}

function getItemLabel(item: MinuteTimelineItem, mode: HistoryTimelineDisplayMode) {
  return mode === "category" ? item.categoryLabel : item.displayName;
}

function selectDominantMinuteItem(bucket: MinuteTimelineBucket) {
  return Array.from(bucket.items.values()).sort((left, right) => (
    right.duration - left.duration
    || left.firstSeenAt - right.firstSeenAt
    || left.key.localeCompare(right.key)
  ))[0];
}

function buildMinuteSegment(
  bucket: MinuteTimelineBucket,
  viewportStartMs: number,
  viewportDurationMs: number,
  mode: HistoryTimelineDisplayMode,
) {
  const dominant = selectDominantMinuteItem(bucket);
  const startTime = bucket.activeStartTime ?? bucket.startTime;
  const endTime = bucket.activeEndTime ?? bucket.endTime;
  if (!dominant || endTime <= startTime) {
    return null;
  }

  const alternateLabels = Array.from(bucket.items.values())
    .filter((item) => item.key !== dominant.key)
    .sort((left, right) => (
      right.duration - left.duration
      || left.firstSeenAt - right.firstSeenAt
      || left.key.localeCompare(right.key)
    ))
    .map((item) => getItemLabel(item, mode));
  const titleSamples = dominant.titleSampleDetails.map((sample) => sample.title);
  const startRatio = clampRatio((startTime - viewportStartMs) / viewportDurationMs);
  const endRatio = clampRatio((endTime - viewportStartMs) / viewportDurationMs);

  return {
    id: `${dominant.key}-${bucket.startTime}-${bucket.endTime}`,
    sourceSessionId: dominant.sourceSessionIds[0] ?? 0,
    timelineKey: dominant.key,
    appKey: dominant.appKey,
    exeName: dominant.exeName,
    displayName: dominant.displayName,
    displayTitle: dominant.displayTitle,
    category: dominant.category,
    categoryLabel: dominant.categoryLabel,
    startTime,
    endTime,
    duration: dominant.duration,
    startRatio,
    endRatio,
    widthRatio: Math.max(0, endRatio - startRatio),
    titleSamples,
    titleSampleDetails: dominant.titleSampleDetails,
    alternateLabels,
    isLive: dominant.isLive,
  } satisfies HistoryTimelineSegment;
}

function mergeContiguousDominantMinuteSegments(
  segments: HistoryTimelineSegment[],
  mergeThresholdMs: number,
) {
  const merged: HistoryTimelineSegment[] = [];

  for (const segment of segments) {
    const current = merged[merged.length - 1];
    if (!current) {
      merged.push(segment);
      continue;
    }

    const gapMs = segment.startTime - current.endTime;
    if (segment.timelineKey === current.timelineKey && gapMs >= 0 && gapMs <= mergeThresholdMs) {
      merged[merged.length - 1] = mergeAdjacentTimelineSegments(current, segment);
      continue;
    }

    merged.push(segment);
  }

  return merged;
}

function keepVisibleTimelineSegments(segments: HistoryTimelineSegment[]) {
  return segments.filter((segment) => segment.duration >= MIN_VISIBLE_TIMELINE_SEGMENT_MS);
}

function buildDominantMinuteSegments(
  segments: HistoryTimelineSegment[],
  dayStartMs: number,
  visibleEndMs: number,
  viewport: HistoryTimelineViewport,
  mode: HistoryTimelineDisplayMode,
  mergeThresholdMs: number,
) {
  const viewportDurationMs = Math.max(1, viewport.endMs - viewport.startMs);
  const timelineEndMs = Math.min(visibleEndMs, viewport.endMs);
  const minuteSegments = buildMinuteBuckets(segments, dayStartMs, timelineEndMs, mode)
    .map((bucket) => buildMinuteSegment(bucket, viewport.startMs, viewportDurationMs, mode))
    .filter((segment): segment is HistoryTimelineSegment => Boolean(segment));

  return keepVisibleTimelineSegments(
    mergeContiguousDominantMinuteSegments(minuteSegments, mergeThresholdMs),
  );
}

function buildLegendItems(
  segments: HistoryTimelineSegment[],
  mode: HistoryTimelineDisplayMode,
): HistoryTimelineLegendItem[] {
  const totalDuration = segments.reduce((total, segment) => total + segment.duration, 0);
  const groups = new Map<string, HistoryTimelineLegendItem>();

  for (const segment of segments) {
    const key = mode === "category" ? segment.category : segment.appKey;
    const existing = groups.get(key);

    if (existing) {
      existing.duration += segment.duration;
      continue;
    }

    groups.set(key, {
      key,
      label: mode === "category" ? segment.categoryLabel : segment.displayName,
      duration: segment.duration,
      percentage: 0,
      category: segment.category,
      exeName: segment.exeName,
    });
  }

  return Array.from(groups.values())
    .map((item) => ({
      ...item,
      percentage: totalDuration > 0 ? (item.duration / totalDuration) * 100 : 0,
    }))
    .sort((a, b) => b.duration - a.duration);
}

export function buildHistoryTimelineViewModel({
  sessions,
  selectedDate,
  nowMs,
  mode,
  mergeThresholdSecs = 0,
  viewport: requestedViewport,
}: BuildHistoryTimelineViewModelParams): HistoryTimelineViewModel {
  const { dayStartMs, dayEndMs } = getFullDayRange(selectedDate);
  const viewport = requestedViewport ?? normalizeHistoryTimelineViewport({
    selectedDate,
    zoomHours: 24,
    requestedStartMs: dayStartMs,
  });
  const visibleEndMs = resolveVisibleEndMs(selectedDate, nowMs, dayStartMs, dayEndMs);
  const mergeThresholdMs = Math.max(0, mergeThresholdSecs) * 1000;
  const rawSegments = sessions
    .map((session) => buildSegment(session, dayStartMs, dayEndMs, visibleEndMs, viewport))
    .filter((segment): segment is HistoryTimelineSegment => Boolean(segment))
    .sort((a, b) => a.startTime - b.startTime || a.endTime - b.endTime);
  const segments = buildDominantMinuteSegments(
    rawSegments,
    dayStartMs,
    visibleEndMs,
    viewport,
    mode,
    mergeThresholdMs,
  );
  const viewportDurationMs = Math.max(1, viewport.endMs - viewport.startMs);

  return {
    segments,
    legendItems: buildLegendItems(segments, mode),
    axisTicks: buildAxisTicks(viewport, dayEndMs),
    dayStartMs,
    dayEndMs,
    viewportStartMs: viewport.startMs,
    viewportEndMs: viewport.endMs,
    viewportDurationMs,
    zoomHours: viewport.zoomHours,
    visibleEndMs,
    visibleEndRatio: clampRatio((visibleEndMs - dayStartMs) / DAY_MS),
  };
}

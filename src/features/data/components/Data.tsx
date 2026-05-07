import { type CSSProperties, useEffect, useMemo, useRef, useState } from "react";
import { BarChart3, ChevronLeft, ChevronRight, Clock3 } from "lucide-react";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, XAxis, YAxis } from "recharts";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import {
  getEarliestSessionStartTime,
  getSessionsInRange,
  type HistorySession,
} from "../../../platform/persistence/sessionReadRepository.ts";
import QuietChartTooltip from "../../../shared/components/QuietChartTooltip";
import QuietPageHeader from "../../../shared/components/QuietPageHeader";
import QuietTooltip from "../../../shared/components/QuietTooltip";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking";
import {
  formatChartHours,
  formatDuration,
} from "../../history/services/historyFormatting";
import {
  buildHistoryReadModel,
  type HistorySnapshot,
} from "../../history/services/historyReadModel";
import {
  getHistorySnapshotCache,
  setHistorySnapshotCache,
} from "../../history/services/historySnapshotCache";

interface Props {
  refreshKey?: number;
  trackerHealth: TrackerHealthSnapshot;
  loadHistorySnapshot: (date: Date, rollingDayCount?: number) => Promise<HistorySnapshot>;
  mappingVersion?: number;
}

interface HeatmapCell {
  key: string;
  date: string;
  duration: number;
  intensity: number;
  isFuture: boolean;
  isOutsideYear: boolean;
  label: string;
}

interface HeatmapWeek {
  key: string;
  monthLabel: string;
  cells: HeatmapCell[];
}

type HeatmapSelection = "recent" | number;

const HEATMAP_WEEKDAYS = ["一", "", "三", "", "五", "", "日"] as const;
const RECENT_HEATMAP_WEEK_COUNT = 53;
const HEATMAP_LOADING_HEIGHT = 104;
const heatmapSessionCache = new Map<string, HistorySession[]>();
let earliestSessionStartTimeCache: number | null | undefined;

function startOfLocalDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function addDays(date: Date, delta: number) {
  const next = new Date(date);
  next.setDate(next.getDate() + delta);
  return next;
}

function toDateKey(date: Date) {
  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  return `${year}-${month}-${day}`;
}

function formatHeatmapDateLabel(dateKey: string) {
  const date = new Date(`${dateKey}T00:00:00`);
  return date.toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" });
}

function formatHeatmapMonthLabel(date: Date) {
  return `${date.getMonth() + 1}月`;
}

function buildYearOptions(earliestStartTime: number | null, currentYear: number) {
  const earliestYear = earliestStartTime ? new Date(earliestStartTime).getFullYear() : currentYear;
  const firstYear = Math.min(earliestYear, currentYear);
  return Array.from(
    { length: currentYear - firstYear + 1 },
    (_, index) => currentYear - index,
  );
}

function getHeatmapRange(selection: HeatmapSelection, nowMs: number) {
  if (selection === "recent") {
    const todayStart = startOfLocalDay(new Date(nowMs));
    const mondayOffset = (todayStart.getDay() + 6) % 7;
    const currentWeekStart = addDays(todayStart, -mondayOffset);
    return {
      start: addDays(currentWeekStart, -(RECENT_HEATMAP_WEEK_COUNT - 1) * 7),
      end: addDays(currentWeekStart, 7),
      weekCount: RECENT_HEATMAP_WEEK_COUNT,
    };
  }

  const yearStart = new Date(selection, 0, 1);
  const nextYearStart = new Date(selection + 1, 0, 1);
  const mondayOffset = (yearStart.getDay() + 6) % 7;
  const heatmapStart = addDays(yearStart, -mondayOffset);
  const lastYearDay = addDays(nextYearStart, -1);
  const lastWeekEndOffset = 6 - ((lastYearDay.getDay() + 6) % 7);
  const heatmapEnd = addDays(lastYearDay, lastWeekEndOffset + 1);

  return {
    start: heatmapStart,
    end: heatmapEnd,
    weekCount: Math.ceil((heatmapEnd.getTime() - heatmapStart.getTime()) / (7 * 24 * 60 * 60 * 1000)),
  };
}

function getHeatmapSelectionKey(selection: HeatmapSelection, nowMs: number) {
  const range = getHeatmapRange(selection, nowMs);
  return `${selection}:${toDateKey(range.start)}:${toDateKey(range.end)}`;
}

function buildActivityHeatmap(
  sessions: HistorySession[],
  selection: HeatmapSelection,
  nowMs: number,
): HeatmapWeek[] {
  const { start: heatmapStart, weekCount } = getHeatmapRange(selection, nowMs);
  const todayStart = startOfLocalDay(new Date(nowMs));
  const dayBuckets = new Map<string, number>();

  for (let dayIndex = 0; dayIndex < weekCount * 7; dayIndex += 1) {
    dayBuckets.set(toDateKey(addDays(heatmapStart, dayIndex)), 0);
  }

  for (const session of sessions) {
    const sessionStart = session.startTime;
    const sessionEnd = session.endTime ?? nowMs;
    if (sessionEnd <= sessionStart) continue;

    let cursor = startOfLocalDay(new Date(sessionStart));
    while (cursor.getTime() < sessionEnd) {
      const dayStart = cursor.getTime();
      const dayEnd = dayStart + 24 * 60 * 60 * 1000;
      const clippedStart = Math.max(sessionStart, dayStart);
      const clippedEnd = Math.min(sessionEnd, dayEnd);
      const key = toDateKey(cursor);
      const previous = dayBuckets.get(key);

      if (previous !== undefined && clippedEnd > clippedStart) {
        dayBuckets.set(key, previous + clippedEnd - clippedStart);
      }

      cursor = addDays(cursor, 1);
    }
  }

  const maxDuration = Math.max(1, ...Array.from(dayBuckets.values()));

  return Array.from({ length: weekCount }, (_, weekIndex) => {
    const weekStart = addDays(heatmapStart, weekIndex * 7);
    const monthStartInWeek = Array.from({ length: 7 }, (_, weekdayIndex) => addDays(weekStart, weekdayIndex))
      .find((date) => (selection === "recent" || date.getFullYear() === selection) && date.getDate() === 1);
    return {
      key: toDateKey(weekStart),
      monthLabel: monthStartInWeek ? formatHeatmapMonthLabel(monthStartInWeek) : "",
      cells: Array.from({ length: 7 }, (_, weekdayIndex) => {
        const date = addDays(weekStart, weekdayIndex);
        const dateKey = toDateKey(date);
        const duration = dayBuckets.get(dateKey) ?? 0;
        const isFuture = date.getTime() > todayStart.getTime();
        const isOutsideYear = selection !== "recent" && date.getFullYear() !== selection;
        return {
          key: dateKey,
          date: dateKey,
          duration,
          isFuture,
          isOutsideYear,
          intensity: duration <= 0 || isFuture || isOutsideYear ? 0 : Math.max(0.16, duration / maxDuration),
          label: `${formatHeatmapDateLabel(dateKey)} · ${isFuture ? "未开始" : formatDuration(duration)}`,
        };
      }),
    };
  });
}

export default function Data({
  refreshKey = 0,
  trackerHealth,
  loadHistorySnapshot,
  mappingVersion = 0,
}: Props) {
  const today = new Date();
  const currentYear = today.getFullYear();
  const cachedSnapshot = getHistorySnapshotCache(today);
  const initialHeatmapCacheKey = getHeatmapSelectionKey("recent", Date.now());
  const [rawSnapshot, setRawSnapshot] = useState<HistorySnapshot | null>(cachedSnapshot);
  const [selectedHeatmapView, setSelectedHeatmapView] = useState<HeatmapSelection>("recent");
  const [earliestStartTime, setEarliestStartTime] = useState<number | null>(earliestSessionStartTimeCache ?? null);
  const [yearSessions, setYearSessions] = useState<HistorySession[]>(
    () => heatmapSessionCache.get(initialHeatmapCacheKey) ?? [],
  );
  const [heatmapLoading, setHeatmapLoading] = useState(!heatmapSessionCache.has(initialHeatmapCacheKey));
  const [hasFetchedHeatmapOnce, setHasFetchedHeatmapOnce] = useState(heatmapSessionCache.has(initialHeatmapCacheKey));
  const [nowMs, setNowMs] = useState(() => cachedSnapshot?.fetchedAtMs ?? Date.now());
  const [loading, setLoading] = useState(!cachedSnapshot);
  const [hasFetchedOverviewOnce, setHasFetchedOverviewOnce] = useState(Boolean(cachedSnapshot));
  const hasLoadedRef = useRef(Boolean(cachedSnapshot));
  const initialRefreshKeyRef = useRef(refreshKey);
  const hasFetchedOverviewOnceRef = useRef(Boolean(cachedSnapshot));
  const hasFetchedHeatmapOnceRef = useRef(heatmapSessionCache.has(initialHeatmapCacheKey));

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      const cached = getHistorySnapshotCache(new Date());

      if (cached) {
        if (!hasFetchedOverviewOnceRef.current) {
          setRawSnapshot(cached);
          setNowMs(cached.fetchedAtMs);
        }
        hasFetchedOverviewOnceRef.current = true;
        setHasFetchedOverviewOnce(true);
        setLoading(false);
      }

      if (cached && hasLoadedRef.current && refreshKey === initialRefreshKeyRef.current) {
        return;
      }

      if (!hasLoadedRef.current && !cached) {
        setLoading(!hasFetchedOverviewOnceRef.current);
      }

      try {
        const snapshot = await loadHistorySnapshot(new Date());
        if (cancelled) return;
        setHistorySnapshotCache(snapshot, new Date());
        setRawSnapshot(snapshot);
        setNowMs(snapshot.fetchedAtMs);
        hasFetchedOverviewOnceRef.current = true;
        setHasFetchedOverviewOnce(true);
        hasLoadedRef.current = true;
      } finally {
        if (!cancelled) {
          setLoading(false);
        }
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [loadHistorySnapshot, refreshKey]);

  useEffect(() => {
    let cancelled = false;
    const loadYear = async () => {
      const nowForRange = Date.now();
      const range = getHeatmapRange(selectedHeatmapView, nowForRange);
      const cacheKey = getHeatmapSelectionKey(selectedHeatmapView, nowForRange);
      const cachedSessions = heatmapSessionCache.get(cacheKey);

      if (cachedSessions) {
        setYearSessions(cachedSessions);
        hasFetchedHeatmapOnceRef.current = true;
        setHasFetchedHeatmapOnce(true);
        setHeatmapLoading(false);
      } else {
        setHeatmapLoading(!hasFetchedHeatmapOnceRef.current);
      }

      try {
        const [earliest, sessions] = await Promise.all([
          earliestSessionStartTimeCache === undefined
            ? getEarliestSessionStartTime()
            : Promise.resolve(earliestSessionStartTimeCache),
          getSessionsInRange(range.start.getTime(), range.end.getTime()),
        ]);
        if (cancelled) return;

        earliestSessionStartTimeCache = earliest;
        heatmapSessionCache.set(cacheKey, sessions);
        setEarliestStartTime(earliest);
        setYearSessions(sessions);
        hasFetchedHeatmapOnceRef.current = true;
        setHasFetchedHeatmapOnce(true);

        if (earliest) {
          const earliestYear = new Date(earliest).getFullYear();
          if (selectedHeatmapView !== "recent" && selectedHeatmapView < earliestYear) {
            setSelectedHeatmapView(earliestYear);
          }
        }
      } finally {
        if (!cancelled) {
          setHeatmapLoading(false);
        }
      }
    };

    void loadYear();
    return () => {
      cancelled = true;
    };
  }, [selectedHeatmapView, refreshKey]);

  const viewModel = useMemo(() => {
    if (!rawSnapshot) return null;
    return buildHistoryReadModel({
      daySessions: rawSnapshot.daySessions,
      weeklySessions: rawSnapshot.weeklySessions,
      selectedDate: today,
      nowMs,
      trackerHealth,
      minSessionSecs: 0,
      mergeThresholdSecs: 0,
    });
  }, [mappingVersion, nowMs, rawSnapshot, today, trackerHealth]);
  const heatmapRows = useMemo(() => (
    buildActivityHeatmap(yearSessions, selectedHeatmapView, nowMs)
  ), [nowMs, selectedHeatmapView, yearSessions]);
  const yearOptions = useMemo(
    () => buildYearOptions(earliestStartTime, currentYear),
    [currentYear, earliestStartTime],
  );
  const heatmapViewOptions = useMemo<HeatmapSelection[]>(
    () => ["recent", ...yearOptions],
    [yearOptions],
  );
  const selectedHeatmapViewIndex = heatmapViewOptions.findIndex((option) => option === selectedHeatmapView);
  const canSelectOlderHeatmapView = selectedHeatmapViewIndex >= 0
    && selectedHeatmapViewIndex < heatmapViewOptions.length - 1;
  const canSelectNewerHeatmapView = selectedHeatmapViewIndex > 0;
  const selectAdjacentHeatmapView = (delta: number) => {
    if (selectedHeatmapViewIndex < 0) return;
    const nextView = heatmapViewOptions[selectedHeatmapViewIndex + delta];
    if (nextView !== undefined) {
      setSelectedHeatmapView(nextView);
    }
  };
  const selectedHeatmapViewLabel = selectedHeatmapView === "recent"
    ? UI_TEXT.data.recentYear
    : String(selectedHeatmapView);

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 md:gap-5 overflow-y-auto pr-1 custom-scrollbar">
      <QuietPageHeader
        icon={<BarChart3 size={18} />}
        title={UI_TEXT.data.title}
        subtitle={UI_TEXT.data.subtitle}
      />

      <div className="data-overview-grid">
        <div className="qp-panel p-5 md:p-6 data-trend-panel">
          <h3 className="font-semibold text-[var(--qp-text-primary)] text-sm">{UI_TEXT.data.pastSevenDays}</h3>
          {(loading && !hasFetchedOverviewOnce) || !viewModel ? (
            <div className="data-trend-chart flex items-center justify-center text-[var(--qp-text-tertiary)] text-xs">
              {UI_TEXT.history.loading}
            </div>
          ) : (
            <div className="pt-4">
              <div className="data-trend-chart">
                <ResponsiveContainer width="100%" height="100%">
                <AreaChart data={viewModel.chartData} margin={{ top: 8, right: 22, left: -18, bottom: 0 }}>
                  <CartesianGrid strokeDasharray="3 3" stroke="rgba(156, 168, 186, 0.25)" />
                  <XAxis
                    dataKey="day"
                    tick={{ fontSize: 11, fill: "var(--qp-text-tertiary)" }}
                    axisLine={false}
                    tickLine={false}
                    interval={0}
                  />
                  <YAxis
                    tick={{ fontSize: 11, fill: "var(--qp-text-tertiary)" }}
                    axisLine={false}
                    tickLine={false}
                    interval={0}
                    ticks={viewModel.chartAxis.ticks}
                    domain={[0, viewModel.chartAxis.domainMax]}
                    tickFormatter={(value) => formatChartHours(Number(value))}
                  />
                  <QuietChartTooltip
                    formatter={(value) => [
                      formatDuration(Number(value) * 3600000),
                      "时长",
                    ]}
                  />
                  <Area
                    type="monotone"
                    dataKey="hours"
                    stroke="var(--qp-accent-default)"
                    strokeWidth={2}
                    fill="var(--qp-accent-default)"
                    fillOpacity={0.12}
                    dot={{ fill: "var(--qp-accent-default)", r: 3 }}
                  isAnimationActive={false}
                />
              </AreaChart>
                </ResponsiveContainer>
              </div>
            </div>
          )}
        </div>

        <div className="data-metric-stack">
          <div className="qp-panel p-5 data-metric-card">
            <div className="flex items-center gap-2 text-[var(--qp-text-secondary)]">
              <Clock3 size={14} />
              <span className="text-xs font-semibold">7 日总时长</span>
            </div>
            <div className="mt-3 text-xl font-semibold tabular-nums text-[var(--qp-text-primary)]">
              {viewModel ? formatDuration(viewModel.weekly.reduce((sum, item) => sum + item.totalDuration, 0)) : "-"}
            </div>
          </div>
        </div>
      </div>

      <div className="grid grid-cols-1 gap-4 md:grid-cols-3 md:gap-5">
        <div className="qp-panel p-5 md:col-span-3">
          <div className="flex items-center justify-between gap-3">
            <div>
              <h3 className="font-semibold text-[var(--qp-text-primary)] text-sm">{UI_TEXT.data.activityHeatmap}</h3>
              <p className="mt-1 text-[11px] text-[var(--qp-text-tertiary)]">
                {selectedHeatmapViewLabel} · {UI_TEXT.data.activityHeatmapHint}
              </p>
            </div>
            <div className="data-heatmap-header-actions">
              <div className="data-heatmap-range-control" aria-label="选择热力图范围">
                <button
                  type="button"
                  onClick={() => selectAdjacentHeatmapView(1)}
                  disabled={!canSelectOlderHeatmapView}
                  className="qp-control data-heatmap-range-arrow"
                  aria-label="查看更早范围"
                >
                  <ChevronLeft size={14} />
                </button>
                <button
                  type="button"
                  className="qp-status data-heatmap-range-label"
                  disabled
                >
                  {selectedHeatmapViewLabel}
                </button>
                <button
                  type="button"
                  onClick={() => selectAdjacentHeatmapView(-1)}
                  disabled={!canSelectNewerHeatmapView}
                  className="qp-control data-heatmap-range-arrow"
                  aria-label="查看更新范围"
                >
                  <ChevronRight size={14} />
                </button>
              </div>
              <div className="hidden items-center gap-1.5 text-[10px] font-medium text-[var(--qp-text-tertiary)] sm:flex">
                <span>{UI_TEXT.data.less}</span>
                <span className="data-heatmap-swatch data-heatmap-level-0" />
                <span className="data-heatmap-swatch data-heatmap-level-1" />
                <span className="data-heatmap-swatch data-heatmap-level-2" />
                <span className="data-heatmap-swatch data-heatmap-level-3" />
                <span className="data-heatmap-swatch data-heatmap-level-4" />
                <span>{UI_TEXT.data.more}</span>
              </div>
            </div>
          </div>

          {heatmapLoading && !hasFetchedHeatmapOnce ? (
            <div
              className="mt-5 flex items-center justify-center text-[var(--qp-text-tertiary)] text-xs"
              style={{ height: HEATMAP_LOADING_HEIGHT }}
            >
              {UI_TEXT.history.loading}
            </div>
          ) : (
            <div className="data-heatmap data-heatmap-calendar mt-5">
              <div className="data-heatmap-content">
                <div className="data-heatmap-scroll">
                  <div className="data-heatmap-months" aria-hidden>
                    <span />
                    {heatmapRows.map((week) => (
                      <span key={week.key}>{week.monthLabel}</span>
                    ))}
                  </div>
                  <div className="data-heatmap-body" aria-label={UI_TEXT.data.activityHeatmap}>
                    <div className="data-heatmap-weekdays" aria-hidden>
                      {HEATMAP_WEEKDAYS.map((weekday, index) => (
                        <span key={`${weekday}-${index}`}>{weekday}</span>
                      ))}
                    </div>
                    <div className="data-heatmap-weeks">
                      {heatmapRows.map((week) => (
                        <div key={week.key} className="data-heatmap-week">
                          {week.cells.map((cell) => (
                            <QuietTooltip
                              key={cell.key}
                              label={cell.label}
                              placement="top"
                              className="data-heatmap-tooltip-anchor"
                            >
                              <span
                                className={`data-heatmap-cell ${
                                  cell.isFuture ? "data-heatmap-cell-future" : ""
                                } ${cell.isOutsideYear ? "data-heatmap-cell-outside" : ""}`}
                                style={{ "--heatmap-intensity": cell.intensity } as CSSProperties}
                              />
                            </QuietTooltip>
                          ))}
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

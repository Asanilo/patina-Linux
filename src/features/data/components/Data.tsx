import {
  type CSSProperties,
  type MouseEvent,
  lazy,
  Suspense,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { BarChart3, CalendarDays, ChevronLeft, ChevronRight, Clock3 } from "lucide-react";
import { Area, AreaChart, CartesianGrid, ResponsiveContainer, XAxis, YAxis } from "recharts";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import type { AppLanguage } from "../../../shared/settings/appSettings.ts";
import {
  buildDataTrendViewModel,
  buildActivityHeatmap,
  buildYearOptions,
  getCachedDataHeatmapSessions,
  getCachedEarliestSessionStartTime,
  type DataAppTrendViewModel,
  type DataTrendViewModel,
  type AggregateSessionRecord,
  type HeatmapWeek,
  type HeatmapSelection,
  loadDataHeatmapSnapshot,
} from "../services/dataReadModel.ts";
import {
  getCachedDataBootstrapSnapshot,
  loadPersistedDataBootstrapSnapshot,
  saveDataBootstrapSnapshot,
  type DataBootstrapSnapshot,
} from "../services/dataBootstrapSnapshot.ts";
import { prewarmDataFirstScreen } from "../services/dataFirstScreenPrewarm.ts";
import QuietChartTooltip from "../../../shared/components/QuietChartTooltip";
import QuietPageHeader from "../../../shared/components/QuietPageHeader";
import QuietSegmentedFilter from "../../../shared/components/QuietSegmentedFilter";
import QuietTooltip from "../../../shared/components/QuietTooltip";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking";
import {
  formatChartHours,
  formatDuration,
} from "../../history/services/historyFormatting";
import { resolveTrendDateFromChartEvent } from "../services/dataChartInteraction.ts";
import type { DataTrendSnapshot } from "../services/dataTrendSnapshot.ts";
import type { DataTrendRangeSelection } from "../services/dataTrendRange.ts";
import { useDataTrendSnapshot } from "../hooks/useDataTrendSnapshot.ts";
import { useDataChartInitialDimension } from "../hooks/useDataChartInitialDimension.ts";
import DataTrendRangeControl from "./DataTrendRangeControl.tsx";
import type { DestinationDetailOpenRequest } from "../../destination/types.ts";

const DataDestinationTrendPanel = lazy(() => import("./DataDestinationTrendPanel.tsx"));

interface Props {
  icons: Record<string, string>;
  refreshKey?: number;
  trackerHealth: TrackerHealthSnapshot;
  loadDataTrendSnapshot: (selection: DataTrendRangeSelection, nowMs?: number) => Promise<DataTrendSnapshot>;
  mappingVersion?: number;
  onOpenHistoryDate?: (dateKey: string) => void;
  onOpenDestinationDetail?: (request: DestinationDetailOpenRequest) => void;
  uiLanguage: AppLanguage;
  webEnabled?: boolean;
}

const DATA_TREND_X_AXIS_MIN_TICK_GAP = 24;
const HEATMAP_WEEKDAY_COUNT = 7;
type HeatmapGranularity = "daily" | "weekly";

function formatHeatmapShortDate(dateKey: string) {
  return dateKey.slice(5).replace("-", "/");
}

function buildWeeklyHeatmapCells(rows: HeatmapWeek[]) {
  const weeklyCells = rows.map((week) => {
    const inRangeCells = week.cells.filter((cell) => !cell.isOutsideYear);
    const visibleCells = inRangeCells.filter((cell) => !cell.isFuture);
    const duration = visibleCells.reduce((total, cell) => total + cell.duration, 0);
    const labelCells = visibleCells.length > 0
      ? visibleCells
      : inRangeCells.length > 0
        ? inRangeCells
        : week.cells;
    const firstCell = labelCells[0];
    const lastCell = labelCells[labelCells.length - 1];
    const dateLabel = firstCell && lastCell
      ? `${formatHeatmapShortDate(firstCell.date)} - ${formatHeatmapShortDate(lastCell.date)}`
      : week.key;
    const isOutsideYear = inRangeCells.length === 0;
    const isFuture = !isOutsideYear && visibleCells.length === 0;

    return {
      key: week.key,
      duration,
      intensity: 0,
      isFuture,
      isOutsideYear,
      label: `${dateLabel} · ${isFuture ? UI_TEXT.data.notStarted : formatDuration(duration)}`,
    };
  });
  const maxDuration = Math.max(1, ...weeklyCells.map((cell) => cell.duration));

  return weeklyCells.map((cell) => ({
    ...cell,
    activeRows: cell.duration <= 0 || cell.isFuture || cell.isOutsideYear
      ? 0
      : Math.max(1, Math.ceil((cell.duration / maxDuration) * HEATMAP_WEEKDAY_COUNT)),
    intensity: cell.duration <= 0 || cell.isFuture || cell.isOutsideYear ? 0 : 0.88,
  }));
}

export default function Data({
  icons,
  refreshKey = 0,
  loadDataTrendSnapshot,
  mappingVersion = 0,
  onOpenHistoryDate,
  onOpenDestinationDetail,
  uiLanguage,
  webEnabled = false,
}: Props) {
  const today = new Date();
  const currentYear = today.getFullYear();
  const [selectedTrendRange, setSelectedTrendRange] = useState<DataTrendRangeSelection>({ kind: "rolling", days: 7 });
  const [selectedAppTrendRange, setSelectedAppTrendRange] = useState<DataTrendRangeSelection>({ kind: "rolling", days: 7 });
  const [currentAppTrendViewModel, setCurrentAppTrendViewModel] = useState<DataAppTrendViewModel | null>(null);
  const initialCachedHeatmapSessions = getCachedDataHeatmapSessions("recent", Date.now());
  const [bootstrapSnapshot, setBootstrapSnapshot] = useState<DataBootstrapSnapshot | null>(
    () => getCachedDataBootstrapSnapshot(),
  );
  const overviewTrend = useDataTrendSnapshot({
    selection: selectedTrendRange,
    refreshKey,
    loadSnapshot: loadDataTrendSnapshot,
  });
  const appTrend = useDataTrendSnapshot({
    selection: selectedAppTrendRange,
    refreshKey,
    loadSnapshot: loadDataTrendSnapshot,
  });
  const [selectedHeatmapView, setSelectedHeatmapView] = useState<HeatmapSelection>("recent");
  const [heatmapGranularity, setHeatmapGranularity] = useState<HeatmapGranularity>("daily");
  const [earliestStartTime, setEarliestStartTime] = useState<number | null>(
    getCachedEarliestSessionStartTime() ?? null,
  );
  const [yearSessions, setYearSessions] = useState<AggregateSessionRecord[]>(
    () => initialCachedHeatmapSessions ?? [],
  );
  const [yearSessionsView, setYearSessionsView] = useState<HeatmapSelection | null>(
    initialCachedHeatmapSessions ? "recent" : null,
  );
  const [heatmapLoading, setHeatmapLoading] = useState(!initialCachedHeatmapSessions);
  const overviewTrendChart = useDataChartInitialDimension("overviewTrend");
  const nowMs = overviewTrend.nowMs;
  const lastTrendViewModelRef = useRef<{
    rangeCacheKey: string;
    viewModel: DataTrendViewModel;
  } | null>(null);
  const lastHeatmapRowsRef = useRef<{
    selection: HeatmapSelection;
    rows: ReturnType<typeof buildActivityHeatmap>;
  } | null>(null);
  const hasFetchedHeatmapOnceRef = useRef(Boolean(initialCachedHeatmapSessions));
  const activeTrendDateRef = useRef<string | null>(null);

  useEffect(() => {
    if (bootstrapSnapshot) return;

    let cancelled = false;
    void loadPersistedDataBootstrapSnapshot().then((snapshot) => {
      if (!cancelled) {
        setBootstrapSnapshot(snapshot);
      }
    });

    return () => {
      cancelled = true;
    };
  }, [bootstrapSnapshot]);

  useEffect(() => {
    void prewarmDataFirstScreen({
      mappingVersion,
      reason: "data-opened",
      uiLanguage,
    });
  }, [mappingVersion, uiLanguage]);

  useEffect(() => {
    let cancelled = false;
    const loadYear = async () => {
      const nowForRange = Date.now();
      const cachedSessions = getCachedDataHeatmapSessions(selectedHeatmapView, nowForRange);

      if (cachedSessions) {
        setYearSessions(cachedSessions);
        setYearSessionsView(selectedHeatmapView);
        hasFetchedHeatmapOnceRef.current = true;
        setHeatmapLoading(false);
      } else {
        setHeatmapLoading(true);
      }

      try {
        const snapshot = await loadDataHeatmapSnapshot(selectedHeatmapView, nowForRange);
        if (cancelled) return;

        setEarliestStartTime(snapshot.earliestStartTime);
        setYearSessions(snapshot.sessions);
        setYearSessionsView(selectedHeatmapView);
        hasFetchedHeatmapOnceRef.current = true;

        if (snapshot.earliestStartTime) {
          const earliestYear = new Date(snapshot.earliestStartTime).getFullYear();
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

  const trendViewModel = useMemo(() => {
    if (!overviewTrend.snapshot) return null;
    return buildDataTrendViewModel(overviewTrend.snapshot.sessions, overviewTrend.snapshot.range, overviewTrend.nowMs);
  }, [mappingVersion, overviewTrend.nowMs, overviewTrend.snapshot]);
  if (trendViewModel) {
    lastTrendViewModelRef.current = {
      rangeCacheKey: overviewTrend.resolvedRange.cacheKey,
      viewModel: trendViewModel,
    };
  }
  const matchingBootstrapSnapshot = bootstrapSnapshot
    && bootstrapSnapshot.mappingVersion === mappingVersion
    && bootstrapSnapshot.uiLanguage === uiLanguage
    ? bootstrapSnapshot
    : null;
  const bootstrapTrendViewModel = matchingBootstrapSnapshot?.overviewRangeCacheKey === overviewTrend.resolvedRange.cacheKey
    ? matchingBootstrapSnapshot.overviewTrendViewModel
    : null;
  const visibleTrendViewModel = trendViewModel
    ?? (lastTrendViewModelRef.current?.rangeCacheKey === overviewTrend.resolvedRange.cacheKey
      ? lastTrendViewModelRef.current.viewModel
      : null)
    ?? bootstrapTrendViewModel;
  const bootstrapAppTrendViewModel = matchingBootstrapSnapshot?.appRangeCacheKey === appTrend.resolvedRange.cacheKey
    ? matchingBootstrapSnapshot.appTrendViewModel
    : null;
  const heatmapRows = useMemo(() => (
    buildActivityHeatmap(yearSessions, selectedHeatmapView, nowMs)
  ), [nowMs, selectedHeatmapView, yearSessions]);
  const hasHeatmapRowsForSelectedView = yearSessionsView === selectedHeatmapView;
  if (!heatmapLoading && hasHeatmapRowsForSelectedView) {
    lastHeatmapRowsRef.current = {
      selection: selectedHeatmapView,
      rows: heatmapRows,
    };
  }
  const bootstrapHeatmapRows = matchingBootstrapSnapshot?.heatmapSelection === selectedHeatmapView
    ? matchingBootstrapSnapshot.heatmapRows
    : null;
  const heatmapPlaceholderRows = useMemo(() => (
    buildActivityHeatmap([], selectedHeatmapView, nowMs)
  ), [nowMs, selectedHeatmapView]);
  const canUseBootstrapHeatmap = Boolean(bootstrapHeatmapRows && (heatmapLoading || !hasHeatmapRowsForSelectedView));
  const visibleHeatmapRows = !heatmapLoading && hasHeatmapRowsForSelectedView
    ? heatmapRows
    : lastHeatmapRowsRef.current?.selection === selectedHeatmapView
      ? lastHeatmapRowsRef.current.rows
      : canUseBootstrapHeatmap
    ? bootstrapHeatmapRows!
        : heatmapPlaceholderRows;
  const weeklyHeatmapCells = useMemo(
    () => buildWeeklyHeatmapCells(visibleHeatmapRows),
    [visibleHeatmapRows],
  );
  const weeklyHeatmapCellsByKey = useMemo(
    () => new Map(weeklyHeatmapCells.map((cell) => [cell.key, cell])),
    [weeklyHeatmapCells],
  );
  const heatmapGranularityOptions = useMemo<Array<{ value: HeatmapGranularity; label: string }>>(() => [
    { value: "daily", label: UI_TEXT.data.heatmapDaily },
    { value: "weekly", label: UI_TEXT.data.heatmapWeekly },
  ], [uiLanguage]);
  const selectedHeatmapViewKey = String(selectedHeatmapView);
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
      setHeatmapLoading(true);
      setSelectedHeatmapView(nextView);
    }
  };
  const selectedHeatmapViewLabel = selectedHeatmapView === "recent"
    ? UI_TEXT.data.recentYear
    : String(selectedHeatmapView);
  const canOpenTrendHistory = visibleTrendViewModel?.granularity === "day" && Boolean(onOpenHistoryDate);
  const handleTrendMouseMove = (event: unknown) => {
    activeTrendDateRef.current = canOpenTrendHistory && visibleTrendViewModel
      ? resolveTrendDateFromChartEvent(event, visibleTrendViewModel.chartData)
      : null;
  };
  const handleTrendDoubleClick = () => {
    const dateKey = activeTrendDateRef.current;
    if (dateKey && canOpenTrendHistory) {
      onOpenHistoryDate?.(dateKey);
    }
  };
  const preventChartTextSelection = (event: MouseEvent<HTMLDivElement>, canOpenHistory: boolean) => {
    if (canOpenHistory && event.detail > 1) {
      event.preventDefault();
    }
  };
  const handleTrendDoubleClickCapture = (event: MouseEvent<HTMLDivElement>) => {
    if (!canOpenTrendHistory) {
      return;
    }

    event.preventDefault();
    handleTrendDoubleClick();
  };
  useEffect(() => {
    if (!trendViewModel || !currentAppTrendViewModel) return;
    if (heatmapLoading || yearSessionsView !== selectedHeatmapView) return;
    if (!overviewTrend.snapshot || !appTrend.snapshot) return;

    const snapshot: DataBootstrapSnapshot = {
      createdAtMs: Date.now(),
      overviewRangeCacheKey: overviewTrend.snapshot.range.cacheKey,
      appRangeCacheKey: appTrend.snapshot.range.cacheKey,
      heatmapSelection: selectedHeatmapView,
      mappingVersion,
      uiLanguage,
      overviewTrendViewModel: trendViewModel,
      appTrendViewModel: currentAppTrendViewModel,
      heatmapRows,
      earliestStartTime,
    };

    setBootstrapSnapshot(snapshot);
    void saveDataBootstrapSnapshot(snapshot);
  }, [
    appTrend.snapshot,
    currentAppTrendViewModel,
    earliestStartTime,
    heatmapLoading,
    heatmapRows,
    mappingVersion,
    overviewTrend.snapshot,
    selectedHeatmapView,
    trendViewModel,
    uiLanguage,
    yearSessionsView,
  ]);

  return (
    <div className="flex h-full min-h-0 flex-col gap-4 md:gap-5 overflow-y-auto pr-1 custom-scrollbar">
      <QuietPageHeader
        icon={<BarChart3 size={18} />}
        title={UI_TEXT.data.title}
        subtitle={UI_TEXT.data.subtitle}
      />

      <div className="data-dashboard-grid">
      <div className="data-overview-grid">
        <div className="qp-panel p-5 md:p-6 data-trend-panel">
          <div className="data-trend-header">
            <h3 className="font-semibold text-[var(--qp-text-primary)] text-sm">
              {UI_TEXT.data.activityTrend}
            </h3>
            <div className="data-trend-inline-metrics" aria-label={UI_TEXT.accessibility.data.trendSummary}>
              <div className="data-trend-inline-metric">
                <Clock3 size={13} aria-hidden />
                <span>{visibleTrendViewModel?.metricLabels.total ?? UI_TEXT.data.weeklyTotal}</span>
                <strong>{visibleTrendViewModel ? formatDuration(visibleTrendViewModel.totalDuration) : "-"}</strong>
              </div>
              <div className="data-trend-inline-metric">
                <CalendarDays size={13} aria-hidden />
                <span>{visibleTrendViewModel?.metricLabels.average ?? UI_TEXT.data.dailyAverage}</span>
                <strong>{visibleTrendViewModel ? formatDuration(visibleTrendViewModel.averageDuration) : "-"}</strong>
              </div>
            </div>
            <DataTrendRangeControl
              ariaLabel={UI_TEXT.accessibility.data.trendRange}
              selection={selectedTrendRange}
              onChange={setSelectedTrendRange}
            />
          </div>
          <div className="pt-4">
            {!visibleTrendViewModel ? (
              <div
                className="data-trend-chart data-chart-placeholder flex items-center justify-center text-[var(--qp-text-tertiary)] text-xs"
                aria-hidden="true"
              />
            ) : (
              <div
                ref={overviewTrendChart.chartRef}
                className={`data-trend-chart ${canOpenTrendHistory ? "data-chart-openable" : ""}`}
                onMouseDownCapture={(event) => {
                  preventChartTextSelection(event, canOpenTrendHistory);
                }}
                onDoubleClickCapture={handleTrendDoubleClickCapture}
              >
                <ResponsiveContainer
                  width="100%"
                  height="100%"
                  initialDimension={overviewTrendChart.initialDimension}
                >
                  <AreaChart
                    data={visibleTrendViewModel.chartData}
                    margin={{ top: 8, right: 22, left: -18, bottom: 0 }}
                    onMouseMove={handleTrendMouseMove}
                    onMouseLeave={() => {
                      activeTrendDateRef.current = null;
                    }}
                  >
                    <CartesianGrid strokeDasharray="3 3" stroke="var(--qp-chart-grid)" />
                    <XAxis
                      dataKey="label"
                      tick={{ fontSize: 11, fill: "var(--qp-text-tertiary)" }}
                      axisLine={false}
                      tickLine={false}
                      interval="preserveStartEnd"
                      minTickGap={DATA_TREND_X_AXIS_MIN_TICK_GAP}
                    />
                    <YAxis
                      tick={{ fontSize: 11, fill: "var(--qp-text-tertiary)" }}
                      axisLine={false}
                      tickLine={false}
                      interval={0}
                      ticks={visibleTrendViewModel.chartAxis.ticks}
                      domain={[0, visibleTrendViewModel.chartAxis.domainMax]}
                      tickFormatter={(value) => formatChartHours(Number(value))}
                    />
                    <QuietChartTooltip
                      formatter={(value) => [
                        formatDuration(Number(value) * 3600000),
                        UI_TEXT.data.duration,
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
            )}
          </div>
        </div>

        <div className="qp-panel p-5 md:p-6 data-heatmap-panel">
          <div className="data-heatmap-panel-header">
            <div>
              <h3 className="font-semibold text-[var(--qp-text-primary)] text-sm">{UI_TEXT.data.activityHeatmap}</h3>
              <p className="mt-1 text-[11px] text-[var(--qp-text-tertiary)]">
                {selectedHeatmapViewLabel} · {UI_TEXT.data.activityHeatmapHint}
              </p>
            </div>
            <div className="data-heatmap-header-actions">
              <QuietSegmentedFilter
                value={heatmapGranularity}
                options={heatmapGranularityOptions}
                onChange={setHeatmapGranularity}
                className="data-heatmap-granularity"
              />
              <div className="data-heatmap-range-control" aria-label={UI_TEXT.accessibility.data.heatmapRange}>
                <button
                  type="button"
                  onClick={() => selectAdjacentHeatmapView(1)}
                  disabled={!canSelectOlderHeatmapView}
                  className="qp-control data-heatmap-range-arrow"
                  aria-label={UI_TEXT.accessibility.data.earlierRange}
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
                  aria-label={UI_TEXT.accessibility.data.newerRange}
                >
                  <ChevronRight size={14} />
                </button>
              </div>
            </div>
          </div>

          <div
            className="data-heatmap data-heatmap-calendar mt-5"
          >
              <div className="data-heatmap-content">
                  <div
                    className="data-heatmap-scroll"
                    style={{ "--data-heatmap-week-count": visibleHeatmapRows.length } as CSSProperties}
                  >
                    <div className="data-heatmap-months" aria-hidden>
                      <span />
                      {visibleHeatmapRows.map((week) => (
                        <span key={`${selectedHeatmapViewKey}:${week.key}`}>{week.monthLabel}</span>
                      ))}
                    </div>
                    <div className="data-heatmap-body" aria-label={UI_TEXT.data.activityHeatmap}>
                      <div className="data-heatmap-weekdays" aria-hidden>
                        {UI_TEXT.date.heatmapWeekdays.map((weekday, index) => (
                          <span key={`${weekday}-${index}`}>{weekday}</span>
                        ))}
                      </div>
                      <div className="data-heatmap-weeks">
                        {visibleHeatmapRows.map((week) => {
                          const weeklyCell = weeklyHeatmapCellsByKey.get(week.key);
                          return (
                            <div key={`${selectedHeatmapViewKey}:${week.key}`} className="data-heatmap-week">
                              {week.cells.map((cell, cellIndex) => {
                                const hideRecentDailyFutureCell = heatmapGranularity === "daily"
                                  && selectedHeatmapView === "recent"
                                  && cell.isFuture;
                                if (hideRecentDailyFutureCell) {
                                  return null;
                                }

                                const isDailyFutureCell = heatmapGranularity === "daily" && cell.isFuture;
                                const isUnavailable = isDailyFutureCell || cell.isOutsideYear;
                                const canOpenHistoryDate = !cell.isFuture && !cell.isOutsideYear && Boolean(onOpenHistoryDate);
                                const tooltipLabel = heatmapGranularity === "weekly"
                                  ? weeklyCell?.label ?? cell.label
                                  : cell.label;
                                const isWeeklyFutureCell = heatmapGranularity === "weekly"
                                  && Boolean(weeklyCell?.isFuture);
                                const tooltipDisabled = heatmapGranularity === "weekly"
                                  ? cell.isOutsideYear || isWeeklyFutureCell
                                  : isUnavailable;
                                const isWeeklyFilledCell = heatmapGranularity === "weekly"
                                  && !cell.isOutsideYear
                                  && cellIndex >= HEATMAP_WEEKDAY_COUNT - (weeklyCell?.activeRows ?? 0);
                                const heatmapIntensity = heatmapGranularity === "weekly"
                                  ? isWeeklyFilledCell ? weeklyCell?.intensity ?? 0 : 0
                                  : cell.intensity;
                                return (
                                  <QuietTooltip
                                    key={`${selectedHeatmapViewKey}:${cell.key}`}
                                    label={tooltipLabel}
                                    placement="top"
                                    disabled={tooltipDisabled}
                                    className={`data-heatmap-tooltip-anchor ${
                                      tooltipDisabled ? "data-heatmap-tooltip-anchor-unavailable" : ""
                                    }`}
                                  >
                                    <span
                                      className={`data-heatmap-cell ${
                                        canOpenHistoryDate ? "data-heatmap-cell-openable" : ""
                                      } ${
                                        isDailyFutureCell || isWeeklyFutureCell ? "data-heatmap-cell-future" : ""
                                      } ${cell.isOutsideYear ? "data-heatmap-cell-outside" : ""}`}
                                      onDoubleClick={() => {
                                        if (canOpenHistoryDate) {
                                          onOpenHistoryDate?.(cell.date);
                                        }
                                      }}
                                      data-history-date={canOpenHistoryDate ? cell.date : undefined}
                                      style={{ "--heatmap-intensity": heatmapIntensity } as CSSProperties}
                                    />
                                  </QuietTooltip>
                                );
                              })}
                            </div>
                          );
                        })}
                      </div>
                    </div>
                  </div>
              </div>
            </div>
        </div>
      </div>

      <Suspense fallback={<div className="qp-panel p-5 md:p-6 data-app-panel" aria-hidden="true" />}>
        <DataDestinationTrendPanel
          appTrendNowMs={appTrend.nowMs}
          appTrendRangeCacheKey={appTrend.resolvedRange.cacheKey}
          appTrendSnapshot={appTrend.snapshot}
          bootstrapAppTrendViewModel={bootstrapAppTrendViewModel}
          icons={icons}
          mappingVersion={mappingVersion}
          onAppTrendViewModelChange={setCurrentAppTrendViewModel}
          onOpenDestinationDetail={onOpenDestinationDetail}
          onOpenHistoryDate={onOpenHistoryDate}
          onSelectionChange={setSelectedAppTrendRange}
          refreshKey={refreshKey}
          selection={selectedAppTrendRange}
          uiLanguage={uiLanguage}
          webEnabled={webEnabled}
        />
      </Suspense>
      </div>
    </div>
  );
}

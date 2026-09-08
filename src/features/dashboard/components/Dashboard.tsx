import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { Layers3, Monitor, Minus, TrendingDown, TrendingUp } from "lucide-react";
import { Cell, Pie, PieChart, ResponsiveContainer } from "recharts";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import { formatDashboardDuration } from "../services/dashboardFormatting";
import type { DashboardReadModel } from "../services/dashboardReadModel";
import HourlyActivityChart from "../../../shared/charts/HourlyActivityChart";
import QuietIconAction from "../../../shared/components/QuietIconAction";
import QuietPageHeader from "../../../shared/components/QuietPageHeader";
import type { HourlyActivityChartMode } from "../../../shared/settings/appSettings.ts";
import type { DestinationDetailOpenRequest } from "../../destination/types.ts";

const DashboardTopApplications = lazy(() => import("./DashboardTopApplications.tsx"));

interface Props {
  dashboard: DashboardReadModel;
  icons: Record<string, string>;
  isAfk: boolean;
  isTrackingActive: boolean;
  activeAppName: string | null;
  trackingPaused: boolean;
  hourlyActivityChartMode: HourlyActivityChartMode;
  onHourlyActivityChartModeChange: (mode: HourlyActivityChartMode) => void;
  onOpenDestinationDetail?: (request: DestinationDetailOpenRequest) => void;
}

const FOCUS_CATEGORY_LIMIT = 4;
const FOCUS_CATEGORY_EXPANDED_LIMIT = 6;
const FOCUS_CATEGORY_EXPANDED_WIDTH = 440;

function buildFocusCategoryDist(categoryDist: DashboardReadModel["categoryDist"], limit: number) {
  const visible = categoryDist.slice(0, limit);
  const rest = categoryDist.slice(limit);
  const restValue = rest.reduce((sum, item) => sum + item.value, 0);

  if (restValue <= 0) {
    return visible;
  }

  return [
    ...visible,
    {
      name: UI_TEXT.categories.other,
      value: restValue,
      color: "var(--qp-text-tertiary)",
    },
  ];
}

export default function Dashboard({
  dashboard,
  icons,
  hourlyActivityChartMode,
  onHourlyActivityChartModeChange,
  onOpenDestinationDetail,
}: Props) {
  const {
    totalTrackedTime,
    dayDeltaTrackedTime,
    topApplications,
    hourlyActivity,
    hourlyCategoryActivity,
    categoryDist,
  } = dashboard;
  const dayDeltaDirection = dayDeltaTrackedTime > 0
    ? "increase"
    : dayDeltaTrackedTime < 0
      ? "decrease"
      : "same";
  const dayDeltaLabel = UI_TEXT.dashboard.comparedWithYesterday(
    formatDashboardDuration(Math.abs(dayDeltaTrackedTime)),
    dayDeltaDirection,
  );
  const DayDeltaIcon = dayDeltaDirection === "increase"
    ? TrendingUp
    : dayDeltaDirection === "decrease"
      ? TrendingDown
      : Minus;
  const focusCardRef = useRef<HTMLDivElement | null>(null);
  const [focusCategoryLimit, setFocusCategoryLimit] = useState(FOCUS_CATEGORY_LIMIT);
  const visibleCategoryDist = buildFocusCategoryDist(categoryDist, focusCategoryLimit);

  useEffect(() => {
    const card = focusCardRef.current;
    if (!card) return;

    const updateLimit = (width: number) => {
      setFocusCategoryLimit(
        width >= FOCUS_CATEGORY_EXPANDED_WIDTH ? FOCUS_CATEGORY_EXPANDED_LIMIT : FOCUS_CATEGORY_LIMIT,
      );
    };

    updateLimit(card.getBoundingClientRect().width);
    const observer = new ResizeObserver(([entry]) => {
      updateLimit(entry.contentRect.width);
    });
    observer.observe(card);
    return () => observer.disconnect();
  }, []);

  return (
    <div className="flex flex-col gap-4 md:gap-5 h-full overflow-hidden">
      <QuietPageHeader
        icon={<Monitor size={18} />}
        title={UI_TEXT.dashboard.title}
        subtitle={UI_TEXT.dashboard.subtitle}
      />

      <div className="flex gap-4 md:gap-5 flex-1 min-h-0 overflow-hidden dashboard-workspace">
        <div className="w-5/12 flex flex-col gap-4 md:gap-5 min-h-0 dashboard-left-column">
          <div
            ref={focusCardRef}
            className="qp-panel p-5 md:p-6 relative overflow-hidden shrink-0 min-h-[250px] dashboard-focus-card"
          >
            <h3 className="w-full text-[var(--qp-text-primary)] font-semibold text-sm mb-4">{UI_TEXT.dashboard.focusShare}</h3>
            <div className="dashboard-focus-layout">
              <div className="relative w-full h-[185px] dashboard-focus-chart">
                <ResponsiveContainer width="100%" height="100%">
                  <PieChart>
                    <Pie
                      data={visibleCategoryDist}
                      innerRadius="68%"
                      outerRadius="100%"
                      paddingAngle={4}
                      dataKey="value"
                      startAngle={90}
                      endAngle={-270}
                      stroke="none"
                      isAnimationActive={false}
                    >
                      {visibleCategoryDist.map((item, index) => (
                        <Cell key={`cell-${index}`} fill={item.color || "var(--qp-accent-default)"} />
                      ))}
                    </Pie>
                  </PieChart>
                </ResponsiveContainer>
                <div className="dashboard-focus-total-center absolute inset-0 flex flex-col items-center justify-center pointer-events-none">
                  <span className="text-[22px] font-semibold text-[var(--qp-text-primary)] tabular-nums">
                    {formatDashboardDuration(totalTrackedTime)}
                  </span>
                  <span className="max-w-[88px] truncate text-[11px] font-semibold text-[var(--qp-text-tertiary)] uppercase tracking-[0.06em]">
                    {UI_TEXT.dashboard.total}
                  </span>
                </div>
              </div>

              <div className="dashboard-focus-ranking" aria-label={UI_TEXT.dashboard.focusShare}>
                {visibleCategoryDist.map((cat) => (
                  <div key={cat.name} className="dashboard-focus-ranking-row">
                    <div className="min-w-0 flex items-center gap-2">
                      <span
                        className="dashboard-focus-ranking-dot"
                        style={{ backgroundColor: cat.color || "var(--qp-accent-default)" }}
                      />
                      <span className="truncate font-semibold text-[var(--qp-text-secondary)]">{cat.name}</span>
                    </div>
                    <span className="text-[var(--qp-text-primary)] font-semibold tabular-nums">
                      {formatDashboardDuration(cat.value)}
                    </span>
                  </div>
                ))}
                {visibleCategoryDist.length === 0 && (
                  <div className="text-xs font-medium text-[var(--qp-text-tertiary)]">{UI_TEXT.dashboard.emptyState}</div>
                )}
              </div>
            </div>
            <p className="dashboard-focus-delta text-[11px] font-medium text-[var(--qp-text-tertiary)]">
              <DayDeltaIcon size={12} strokeWidth={2} />
              {dayDeltaLabel}
            </p>
          </div>

          <div className="qp-panel p-5 flex min-h-0 flex-col overflow-hidden dashboard-pulse-card">
            <div className="mb-4 flex items-center justify-between gap-3">
              <h3 className="text-[var(--qp-text-primary)] font-semibold text-sm">
                {UI_TEXT.dashboard.hourlyActivity}
              </h3>
              <QuietIconAction
                icon={<Layers3 size={15} />}
                title={hourlyActivityChartMode === "category"
                  ? UI_TEXT.dashboard.showTotalHourlyActivity
                  : UI_TEXT.dashboard.showHourlyActivityByCategory}
                pressed={hourlyActivityChartMode === "category"}
                className="hourly-chart-mode-toggle dashboard-pulse-mode-toggle"
                showTooltip={false}
                onClick={() => onHourlyActivityChartModeChange(
                  hourlyActivityChartMode === "category" ? "total" : "category",
                )}
              />
            </div>
            <div className="flex-1 min-h-[170px] dashboard-pulse-chart">
              <HourlyActivityChart
                mode={hourlyActivityChartMode}
                hourlyActivity={hourlyActivity}
                hourlyCategoryActivity={hourlyCategoryActivity}
                margin={{ top: 6, right: 12, left: 10, bottom: 4 }}
                padding={{ left: 12, right: 12 }}
              />
            </div>
          </div>
        </div>

        <Suspense
          fallback={(
            <div className="flex-1 qp-panel p-5 md:p-6 min-h-0" aria-hidden="true">
              <header className="flex justify-between items-center mb-5">
                <h3 className="font-semibold text-[var(--qp-text-primary)] text-base">
                  {UI_TEXT.dashboard.topApps}
                </h3>
                <div className="qp-chip px-2.5 py-1 text-[10px] font-semibold text-[var(--qp-text-secondary)]">
                  {UI_TEXT.dashboard.topAppsBadge(topApplications.length)}
                </div>
              </header>
            </div>
          )}
        >
          <DashboardTopApplications
            icons={icons}
            topApplications={topApplications}
            onOpenDestinationDetail={onOpenDestinationDetail}
          />
        </Suspense>
      </div>
    </div>
  );
}

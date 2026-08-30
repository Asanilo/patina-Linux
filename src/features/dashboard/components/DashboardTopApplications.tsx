import { Monitor, PanelRightOpen } from "lucide-react";
import { AppClassification } from "../../../shared/classification/appClassification.ts";
import QuietIconAction from "../../../shared/components/QuietIconAction.tsx";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import { useIconThemeColors } from "../../../shared/hooks/useIconThemeColors.ts";
import { formatLocalDateKey } from "../../../shared/lib/localDate.ts";
import { getDestinationDetailCopy } from "../../destination/destinationDetailCopy.ts";
import {
  createDestinationDetailTarget,
  type DestinationDetailOpenRequest,
} from "../../destination/types.ts";
import {
  formatDashboardDuration,
  type TopApplicationItem,
} from "../services/dashboardFormatting.ts";

interface Props {
  icons: Record<string, string>;
  topApplications: TopApplicationItem[];
  onOpenDestinationDetail?: (request: DestinationDetailOpenRequest) => void;
}

export default function DashboardTopApplications({
  icons,
  topApplications,
  onOpenDestinationDetail,
}: Props) {
  const iconThemeColors = useIconThemeColors(icons);
  const detailCopy = getDestinationDetailCopy();

  return (
    <div className="flex-1 qp-panel p-5 md:p-6 flex flex-col overflow-hidden min-h-0">
      <header className="flex justify-between items-center mb-5">
        <h3 className="font-semibold text-[var(--qp-text-primary)] text-base">
          {UI_TEXT.dashboard.topApps}
        </h3>
        <div className="qp-chip px-2.5 py-1 text-[10px] font-semibold text-[var(--qp-text-secondary)]">
          {UI_TEXT.dashboard.topAppsBadge(topApplications.length)}
        </div>
      </header>

      <div className="flex-1 overflow-y-auto pr-1 md:pr-2 space-y-2.5 custom-scrollbar">
        {topApplications.length === 0 && (
          <div className="h-full flex flex-col items-center justify-center text-[var(--qp-text-tertiary)] gap-2">
            <Monitor size={32} className="opacity-40" />
            <p className="text-sm font-medium mt-2">{UI_TEXT.dashboard.emptyState}</p>
          </div>
        )}
        {topApplications.map((app) => {
          const overrideColor = AppClassification.getUserOverride(app.exeName)?.color;
          const accentColor = overrideColor ?? iconThemeColors[app.exeName] ?? app.color;

          return (
            <div
              key={app.exeName}
              className="flex items-center justify-between px-3.5 py-3 border border-[var(--qp-border-subtle)] bg-[var(--qp-bg-elevated)] rounded-[10px] hover:border-[var(--qp-border-strong)] hover:bg-[var(--qp-bg-panel)] transition-colors cursor-default"
            >
              <div className="flex items-center gap-4 flex-1 min-w-0">
                <div
                  className="w-10 h-10 bg-[var(--qp-bg-panel)] rounded-[8px] flex items-center justify-center border border-[var(--qp-border-subtle)] overflow-hidden p-2"
                  style={{ boxShadow: `0 0 0 2px ${accentColor}22` }}
                >
                  {icons[app.exeName] ? (
                    <img src={icons[app.exeName]} className="w-full h-full object-contain" alt="" />
                  ) : (
                    <div className="text-xs font-semibold opacity-40 text-[var(--qp-text-secondary)]">
                      {app.categoryInitial}
                    </div>
                  )}
                </div>
                <div className="truncate">
                  <div className="font-semibold text-[var(--qp-text-primary)] text-sm truncate">
                    {app.name}
                  </div>
                  <div className="text-[10px] text-[var(--qp-text-tertiary)] font-medium mt-0.5 tabular-nums">
                    {UI_TEXT.dashboard.sharePrefix} {app.percentage}%
                  </div>
                </div>
              </div>

              <div className="ml-4 flex flex-shrink-0 items-center gap-3">
                <div className="text-right">
                  <div className="font-semibold text-[var(--qp-text-primary)] text-sm tabular-nums">
                    {formatDashboardDuration(app.duration)}
                  </div>
                  <div className="w-20 h-1.5 bg-[var(--qp-track-muted)] rounded-full mt-2.5 overflow-hidden">
                    <div
                      className="dashboard-top-app-progress h-full rounded-full"
                      style={{ backgroundColor: accentColor, width: `${app.percentage}%` }}
                    />
                  </div>
                </div>
                {onOpenDestinationDetail ? (
                  <QuietIconAction
                    icon={<PanelRightOpen size={15} aria-hidden />}
                    title={detailCopy.title}
                    className="dashboard-top-app-detail"
                    showTooltip={false}
                    onClick={() => onOpenDestinationDetail({
                      target: createDestinationDetailTarget({
                        mode: "app",
                        key: app.exeName,
                        identityKeys: [app.exeName],
                        displayName: app.name,
                        secondaryText: app.exeName,
                        iconUrl: icons[app.exeName] ?? null,
                        color: accentColor,
                      }),
                      initialDateKey: formatLocalDateKey(new Date()),
                    })}
                  />
                ) : null}
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

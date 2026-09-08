import { useMemo, useState, type CSSProperties } from "react";
import { ChevronLeft, ChevronRight, Minus, Plus } from "lucide-react";
import QuietDatePicker from "../../../shared/components/QuietDatePicker.tsx";
import QuietDialog from "../../../shared/components/QuietDialog.tsx";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import {
  getDestinationDetailTitleRecords,
  type DestinationDetailActivity,
} from "../services/destinationDetailReadModel.ts";
import {
  clampDetailMinSecs,
  DETAIL_MIN_SECS_RANGE,
  readDetailMinSecs,
  readDestinationDetailTimelineZoomHours,
  rememberDestinationDetailTimelineZoomHours,
  saveDetailMinSecs,
} from "../services/destinationDetailPreferenceStorage.ts";
import {
  buildDestinationDetailTimelineAxisTicks,
  buildDestinationDetailTimelineSegments,
  clipDestinationDetailActivitiesToViewport,
  getInitialDestinationDetailTimelineViewport,
  panDestinationDetailTimelineViewport,
  resizeDestinationDetailTimelineViewport,
  type DestinationDetailTimelineViewport,
} from "../services/destinationDetailTimelineViewport.ts";
import {
  getAdjacentDestinationDetailDateKey,
} from "../services/destinationDetailState.ts";
import {
  formatDestinationDuration,
  formatDestinationTime,
} from "../services/destinationTimeFormatting.ts";
import { useDestinationDetail } from "../hooks/useDestinationDetail.ts";
import type {
  DestinationDetailRuntimeContext,
  DestinationDetailTarget,
} from "../types.ts";
import { getDestinationDetailCopy } from "../destinationDetailCopy.ts";

interface Props {
  target: DestinationDetailTarget;
  initialDateKey: string;
  runtime: DestinationDetailRuntimeContext;
  onClose: () => void;
}

function DetailRecordList({
  activity,
  mode,
}: {
  activity: DestinationDetailActivity;
  mode: DestinationDetailTarget["mode"];
}) {
  const copy = getDestinationDetailCopy();
  const records = mode === "web"
    ? activity.records.filter((record) => Boolean(record.title || record.url))
    : getDestinationDetailTitleRecords(activity);
  if (records.length === 0) return null;

  return (
    <details className="destination-detail-record-details">
      <summary>{copy.titleRows(records.length)}</summary>
      <ol>
        {records.map((record) => (
          <li key={record.id}>
            <span className="destination-detail-record-time">
              {formatDestinationTime(record.startTime)} - {formatDestinationTime(record.endTime)}
            </span>
            <span className="destination-detail-record-title">
              {record.title ?? copy.untitled}
            </span>
            {record.url ? (
              <span className="destination-detail-record-url" title={record.url}>{record.url}</span>
            ) : null}
          </li>
        ))}
      </ol>
    </details>
  );
}

export default function DestinationDetailDialog({
  target,
  initialDateKey,
  runtime,
  onClose,
}: Props) {
  const copy = getDestinationDetailCopy();
  const [minimumDurationSecs, setMinimumDurationSecs] = useState(readDetailMinSecs);
  const [preferredZoomHours, setPreferredZoomHours] = useState(
    readDestinationDetailTimelineZoomHours,
  );
  const [viewportState, setViewportState] = useState<{
    identity: string;
    viewport: DestinationDetailTimelineViewport;
  } | null>(null);
  const detail = useDestinationDetail({
    target,
    initialDateKey,
    refreshKey: runtime.refreshKey,
    mappingVersion: runtime.mappingVersion,
    mergeThresholdSecs: runtime.mergeThresholdSecs,
    trackerHealth: runtime.trackerHealth,
  });
  const displayDay = detail.day.viewModel?.dateKey === detail.focusedDateKey
    ? detail.day.viewModel
    : null;
  const timelineIdentity = displayDay
    ? `${target.mode}:${target.key}:${displayDay.dateKey}`
    : null;
  const initialViewport = displayDay
    ? getInitialDestinationDetailTimelineViewport(
      displayDay,
      detail.nowMs,
      preferredZoomHours,
    )
    : null;
  const viewport = timelineIdentity && viewportState?.identity === timelineIdentity
    ? viewportState.viewport
    : initialViewport;
  const minimumDurationMs = minimumDurationSecs * 1_000;
  const activitiesInViewport = useMemo(() => (
    displayDay && viewport
      ? clipDestinationDetailActivitiesToViewport(displayDay.activities, viewport)
      : []
  ), [displayDay, viewport]);
  const visibleActivities = useMemo(() => (
    activitiesInViewport.filter((activity) => activity.duration >= minimumDurationMs)
  ), [activitiesInViewport, minimumDurationMs]);
  const segments = displayDay && viewport
    ? buildDestinationDetailTimelineSegments(displayDay.activities, viewport, minimumDurationMs)
    : [];
  const axisTicks = displayDay && viewport
    ? buildDestinationDetailTimelineAxisTicks(viewport, displayDay.dayStartMs, displayDay.dayEndMs)
    : [];
  const previousDateKey = getAdjacentDestinationDetailDateKey(detail.focusedDateKey, -1);
  const nextDateKey = getAdjacentDestinationDetailDateKey(detail.focusedDateKey, 1);

  const updateViewport = (nextViewport: DestinationDetailTimelineViewport) => {
    if (!timelineIdentity) return;
    setViewportState({ identity: timelineIdentity, viewport: nextViewport });
  };
  const updateZoom = (nextZoomHours: number) => {
    if (!displayDay || !viewport) return;
    setPreferredZoomHours(nextZoomHours);
    rememberDestinationDetailTimelineZoomHours(nextZoomHours);
    updateViewport(resizeDestinationDetailTimelineViewport({
      dayStartMs: displayDay.dayStartMs,
      dayEndMs: displayDay.dayEndMs,
      viewport,
      requestedZoomHours: nextZoomHours,
    }));
  };
  const panViewport = (direction: -1 | 1) => {
    if (!displayDay || !viewport) return;
    updateViewport(panDestinationDetailTimelineViewport({
      dayStartMs: displayDay.dayStartMs,
      dayEndMs: displayDay.dayEndMs,
      viewport,
      deltaMs: direction * viewport.durationMs * 0.5,
    }));
  };
  const updateMinimumDuration = (deltaMinutes: -1 | 1) => {
    const next = clampDetailMinSecs(minimumDurationSecs + deltaMinutes * 60);
    setMinimumDurationSecs(next);
    saveDetailMinSecs(next);
  };

  return (
    <QuietDialog
      open
      title={`${target.displayName} · ${copy.title}`}
      description={target.secondaryText}
      onClose={onClose}
      surfaceClassName="destination-detail-dialog"
      actions={(
        <button type="button" className="qp-button-secondary qp-dialog-action" onClick={onClose}>
          {copy.close}
        </button>
      )}
    >
      <div
        className="destination-detail"
        style={{ "--destination-color": target.color } as CSSProperties}
      >
        <div className="destination-detail-toolbar">
          <div className="destination-detail-date-controls">
            <button
              type="button"
              className="qp-control destination-detail-icon-button"
              aria-label={copy.previousDay}
              disabled={!previousDateKey}
              onClick={() => previousDateKey && detail.setFocusedDateKey(previousDateKey)}
            >
              <ChevronLeft size={15} aria-hidden />
            </button>
            <QuietDatePicker
              value={detail.focusedDateKey}
              onChange={detail.setFocusedDateKey}
              maxDate={detail.todayDateKey}
              ariaLabel={UI_TEXT.date.pickDate}
            />
            <button
              type="button"
              className="qp-control destination-detail-icon-button"
              aria-label={copy.nextDay}
              disabled={!nextDateKey}
              onClick={() => nextDateKey && detail.setFocusedDateKey(nextDateKey)}
            >
              <ChevronRight size={15} aria-hidden />
            </button>
          </div>
          {displayDay ? (
            <div className="destination-detail-summary" aria-label={copy.recordedDuration}>
              <strong>{formatDestinationDuration(displayDay.totalDuration)}</strong>
              <span>{copy.recordedDuration}</span>
            </div>
          ) : null}
        </div>

        {detail.day.status === "error" && !displayDay ? (
          <div className="destination-detail-state" role="alert">
            <span>{copy.dayError}</span>
            <button type="button" className="qp-inline-action" onClick={detail.retryDay}>
              {copy.retry}
            </button>
          </div>
        ) : !displayDay || !viewport ? (
          <div className="destination-detail-state" role="status">{copy.loading}</div>
        ) : (
          <>
            {detail.day.status === "error" ? (
              <div className="destination-detail-inline-error" role="status">
                <span>{copy.dayError}</span>
                <button type="button" className="qp-inline-action" onClick={detail.retryDay}>
                  {copy.retry}
                </button>
              </div>
            ) : null}
            <section className="destination-detail-section" aria-label={copy.timeline}>
              <div className="destination-detail-section-heading">
                <h4>{copy.timeline}</h4>
                <div className="destination-detail-window-controls">
                  <button
                    type="button"
                    className="qp-control destination-detail-icon-button"
                    aria-label={copy.panEarlier}
                    disabled={viewport.startMs <= displayDay.dayStartMs}
                    onClick={() => panViewport(-1)}
                  >
                    <ChevronLeft size={14} aria-hidden />
                  </button>
                  <label className="destination-detail-zoom-control">
                    <span>{copy.zoomHours(Number((viewport.durationMs / 3_600_000).toFixed(1)))}</span>
                    <input
                      type="range"
                      min="1"
                      max="24"
                      step="0.2"
                      value={viewport.durationMs / 3_600_000}
                      aria-label={copy.timelineZoom}
                      onChange={(event) => updateZoom(Number(event.target.value))}
                    />
                  </label>
                  <button
                    type="button"
                    className="qp-control destination-detail-icon-button"
                    aria-label={copy.panLater}
                    disabled={viewport.endMs >= displayDay.dayEndMs}
                    onClick={() => panViewport(1)}
                  >
                    <ChevronRight size={14} aria-hidden />
                  </button>
                </div>
              </div>
              <div className="destination-detail-timeline-track" role="img" aria-label={copy.timelineAria}>
                {segments.map((segment) => (
                  <span
                    key={segment.id}
                    className={`destination-detail-timeline-segment ${segment.current ? "destination-detail-current" : ""}`}
                    style={{
                      left: `${segment.startRatio * 100}%`,
                      width: `${Math.max(0.35, (segment.endRatio - segment.startRatio) * 100)}%`,
                    }}
                    title={`${formatDestinationTime(segment.startTime)} - ${formatDestinationTime(segment.endTime)} · ${formatDestinationDuration(segment.duration)}`}
                  />
                ))}
              </div>
              <div className="destination-detail-axis" aria-hidden>
                {axisTicks.map((tick) => (
                  <span key={`${tick.label}:${tick.ratio}`} style={{ left: `${tick.ratio * 100}%` }}>
                    {tick.label}
                  </span>
                ))}
              </div>
            </section>

            <section className="destination-detail-section destination-detail-record-section">
              <div className="destination-detail-section-heading">
                <h4>{copy.records}</h4>
                <div className="destination-detail-minimum-control" role="group" aria-label={copy.minimumDuration}>
                  <button
                    type="button"
                    className="qp-control destination-detail-icon-button"
                    disabled={minimumDurationSecs <= DETAIL_MIN_SECS_RANGE.min}
                    aria-label={UI_TEXT.accessibility.history.decreaseMinDuration}
                    onClick={() => updateMinimumDuration(-1)}
                  >
                    <Minus size={13} aria-hidden />
                  </button>
                  <span>{copy.minimumMinutes(minimumDurationSecs / 60)}</span>
                  <button
                    type="button"
                    className="qp-control destination-detail-icon-button"
                    disabled={minimumDurationSecs >= DETAIL_MIN_SECS_RANGE.max}
                    aria-label={UI_TEXT.accessibility.history.increaseMinDuration}
                    onClick={() => updateMinimumDuration(1)}
                  >
                    <Plus size={13} aria-hidden />
                  </button>
                </div>
              </div>
              {visibleActivities.length === 0 ? (
                <div className="destination-detail-state" role="status">
                  {activitiesInViewport.length === 0
                    ? displayDay.activities.length === 0 ? copy.noActivity : copy.noActivityInWindow
                    : copy.noActivityAtMinimum(minimumDurationSecs / 60)}
                </div>
              ) : (
                <ol className="destination-detail-activities custom-scrollbar">
                  {visibleActivities.map((activity) => (
                    <li key={activity.id} className="destination-detail-activity">
                      <div className="destination-detail-activity-main">
                        <span className="destination-detail-activity-time">
                          {formatDestinationTime(activity.startTime, displayDay.dayEndMs)} - {formatDestinationTime(activity.endTime, displayDay.dayEndMs)}
                        </span>
                        <strong>{formatDestinationDuration(activity.duration)}</strong>
                        {activity.current ? <span className="qp-status">{copy.current}</span> : null}
                      </div>
                      <DetailRecordList activity={activity} mode={target.mode} />
                    </li>
                  ))}
                </ol>
              )}
            </section>
          </>
        )}
      </div>
    </QuietDialog>
  );
}

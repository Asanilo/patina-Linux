import { useCallback, useEffect, useMemo, useState } from "react";
import { CalendarClock, FolderOpen } from "lucide-react";
import QuietActionRow from "../../../shared/components/QuietActionRow";
import QuietDialog from "../../../shared/components/QuietDialog";
import QuietSelect from "../../../shared/components/QuietSelect";
import QuietSwitch from "../../../shared/components/QuietSwitch";
import QuietTimePicker from "../../../shared/components/QuietTimePicker";
import { getUiTextLanguage, UI_TEXT } from "../../../shared/copy/uiText.ts";
import {
  ScheduledBackupService,
  type ScheduledBackupCadence,
  type ScheduledBackupConfigInput,
  type ScheduledBackupSnapshot,
} from "../services/scheduledBackupService.ts";

function minutesToTime(minutes: number): string {
  return `${String(Math.floor(minutes / 60)).padStart(2, "0")}:${String(minutes % 60).padStart(2, "0")}`;
}

function timeToMinutes(value: string): number {
  const [hours, minutes] = value.split(":").map(Number);
  return hours * 60 + minutes;
}

function formatDateTime(value: number | null): string {
  if (value === null) return UI_TEXT.settings.scheduledBackupNotAvailable;
  const locale = getUiTextLanguage() === "zh-CN" ? "zh-CN" : "en-US";
  return new Intl.DateTimeFormat(locale, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function draftFromSnapshot(snapshot: ScheduledBackupSnapshot): ScheduledBackupConfigInput {
  return {
    enabled: snapshot.config.enabled,
    cadence: snapshot.config.cadence,
    weekday: snapshot.config.weekday,
    localTimeMinutes: snapshot.config.localTimeMinutes,
    targetDir: snapshot.config.targetDir,
  };
}

export default function SettingsScheduledBackupPanel() {
  const [snapshot, setSnapshot] = useState<ScheduledBackupSnapshot | null>(null);
  const [draft, setDraft] = useState<ScheduledBackupConfigInput | null>(null);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const reload = useCallback(async () => {
    try {
      const next = await ScheduledBackupService.load();
      setSnapshot(next);
      setError(null);
    } catch (loadError) {
      console.error("load scheduled backup snapshot failed", loadError);
      setError(UI_TEXT.settings.scheduledBackupLoadFailed);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void reload();
    let disposed = false;
    let unlisten: (() => void) | null = null;
    void ScheduledBackupService.subscribe(() => void reload())
      .then((dispose) => {
        if (disposed) dispose();
        else unlisten = dispose;
      })
      .catch((listenError) => {
        console.error("subscribe scheduled backup changes failed", listenError);
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [reload]);

  const cadenceOptions = useMemo(() => [
    { value: "daily" as const, label: UI_TEXT.settings.scheduledBackupDaily },
    { value: "weekly" as const, label: UI_TEXT.settings.scheduledBackupWeekly },
  ], []);
  const weekdayOptions = useMemo(() => UI_TEXT.settings.scheduledBackupWeekdays.map(
    (label, index) => ({ value: index + 1, label }),
  ), []);

  const openDialog = () => {
    if (!snapshot) return;
    setDraft(draftFromSnapshot(snapshot));
    setError(null);
    setDialogOpen(true);
  };

  const save = async () => {
    if (!draft || saving) return;
    setSaving(true);
    setError(null);
    try {
      const normalizedDraft = {
        ...draft,
        weekday: draft.cadence === "weekly" ? draft.weekday ?? 5 : null,
      };
      const next = await ScheduledBackupService.save(normalizedDraft);
      setSnapshot(next);
      setDraft(draftFromSnapshot(next));
      setDialogOpen(false);
    } catch (saveError) {
      console.error("save scheduled backup configuration failed", saveError);
      setError(UI_TEXT.settings.scheduledBackupSaveFailed);
    } finally {
      setSaving(false);
    }
  };

  const chooseDirectory = async () => {
    if (!draft) return;
    try {
      const selected = await ScheduledBackupService.pickDirectory(draft.targetDir);
      if (selected) setDraft({ ...draft, targetDir: selected });
    } catch (pickError) {
      console.error("pick scheduled backup directory failed", pickError);
      setError(UI_TEXT.settings.scheduledBackupDirectoryFailed);
    }
  };

  const statusLabel = loading
    ? UI_TEXT.common.loading
    : snapshot?.activeRun
      ? UI_TEXT.settings.scheduledBackupRunning
      : snapshot?.config.enabled
        ? UI_TEXT.settings.scheduledBackupEnabled
        : UI_TEXT.settings.scheduledBackupDisabled;

  return (
    <>
      <QuietActionRow className="mt-3">
        <div className="flex flex-col gap-3 md:flex-row md:items-center md:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <CalendarClock size={14} className="text-[var(--qp-text-tertiary)]" />
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
                {UI_TEXT.settings.scheduledBackupTitle}
              </p>
              <span className="qp-status qp-status-muted">{statusLabel}</span>
            </div>
            <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-tertiary)]">
              {snapshot?.config.enabled
                ? UI_TEXT.settings.scheduledBackupNext(formatDateTime(snapshot.nextExecutionAtMs))
                : UI_TEXT.settings.scheduledBackupHint}
            </p>
            {snapshot?.recentFailure && (
              <p className="mt-1 text-xs leading-relaxed text-[var(--qp-danger)]">
                {UI_TEXT.settings.scheduledBackupLastFailure(snapshot.recentFailure.errorMessage ?? "")}
              </p>
            )}
            {error && !dialogOpen && (
              <p className="mt-1 text-xs text-[var(--qp-danger)]">{error}</p>
            )}
          </div>
          <button
            type="button"
            onClick={openDialog}
            disabled={!snapshot || loading}
            className="qp-button-secondary h-8 shrink-0 rounded-[8px] px-3 text-xs font-semibold disabled:opacity-50"
          >
            {UI_TEXT.settings.scheduledBackupConfigure}
          </button>
        </div>
      </QuietActionRow>

      <QuietDialog
        open={dialogOpen && Boolean(draft)}
        title={UI_TEXT.settings.scheduledBackupDialogTitle}
        description={UI_TEXT.settings.scheduledBackupDialogDescription}
        onClose={() => !saving && setDialogOpen(false)}
        closeOnBackdrop={!saving}
        actions={(
          <>
            <button
              type="button"
              onClick={() => setDialogOpen(false)}
              disabled={saving}
              className="qp-button-secondary h-8 rounded-[8px] px-3 text-xs font-semibold disabled:opacity-50"
            >
              {UI_TEXT.common.cancel}
            </button>
            <button
              type="button"
              onClick={() => void save()}
              disabled={saving || !draft?.targetDir.trim()}
              className="qp-button-primary h-8 rounded-[8px] px-3 text-xs font-semibold disabled:opacity-50"
            >
              {saving ? UI_TEXT.common.saving : UI_TEXT.common.save}
            </button>
          </>
        )}
      >
        {draft && (
          <div className="space-y-4">
            <div className="flex items-center justify-between gap-4">
              <div>
                <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
                  {UI_TEXT.settings.scheduledBackupEnableLabel}
                </p>
                <p className="mt-1 text-xs text-[var(--qp-text-tertiary)]">
                  {UI_TEXT.settings.scheduledBackupKeepHint}
                </p>
              </div>
              <QuietSwitch
                checked={draft.enabled}
                disabled={saving}
                ariaLabel={UI_TEXT.settings.scheduledBackupEnableLabel}
                onChange={(enabled) => setDraft({ ...draft, enabled })}
              />
            </div>

            <div className="grid gap-3 sm:grid-cols-3">
              <label className="space-y-1.5">
                <span className="text-xs font-semibold text-[var(--qp-text-secondary)]">
                  {UI_TEXT.settings.scheduledBackupCadenceLabel}
                </span>
                <QuietSelect<ScheduledBackupCadence>
                  value={draft.cadence}
                  options={cadenceOptions}
                  disabled={saving}
                  onChange={(cadence) => setDraft({
                    ...draft,
                    cadence,
                    weekday: cadence === "weekly" ? draft.weekday ?? 5 : null,
                  })}
                />
              </label>
              <label className="space-y-1.5">
                <span className="text-xs font-semibold text-[var(--qp-text-secondary)]">
                  {UI_TEXT.settings.scheduledBackupWeekdayLabel}
                </span>
                <QuietSelect<number>
                  value={draft.weekday ?? 5}
                  options={weekdayOptions}
                  disabled={saving || draft.cadence !== "weekly"}
                  onChange={(weekday) => setDraft({ ...draft, weekday })}
                />
              </label>
              <label className="space-y-1.5">
                <span className="text-xs font-semibold text-[var(--qp-text-secondary)]">
                  {UI_TEXT.settings.scheduledBackupTimeLabel}
                </span>
                <QuietTimePicker
                  value={minutesToTime(draft.localTimeMinutes)}
                  disabled={saving}
                  ariaLabel={UI_TEXT.settings.scheduledBackupTimeLabel}
                  onChange={(value) => setDraft({ ...draft, localTimeMinutes: timeToMinutes(value) })}
                />
              </label>
            </div>

            <div>
              <label className="text-xs font-semibold text-[var(--qp-text-secondary)]" htmlFor="scheduled-backup-dir">
                {UI_TEXT.settings.scheduledBackupDirectoryLabel}
              </label>
              <div className="mt-1.5 flex gap-2">
                <input
                  id="scheduled-backup-dir"
                  value={draft.targetDir}
                  disabled={saving}
                  onChange={(event) => setDraft({ ...draft, targetDir: event.target.value })}
                  className="qp-input min-w-0 flex-1 font-mono text-xs"
                />
                <button
                  type="button"
                  onClick={() => void chooseDirectory()}
                  disabled={saving}
                  title={UI_TEXT.settings.scheduledBackupChooseDirectory}
                  className="qp-button-secondary flex h-9 w-9 shrink-0 items-center justify-center rounded-[8px] disabled:opacity-50"
                >
                  <FolderOpen size={15} />
                </button>
              </div>
            </div>

            {snapshot?.recentSuccess && (
              <p className="text-xs text-[var(--qp-text-tertiary)]">
                {UI_TEXT.settings.scheduledBackupLastSuccess(
                  formatDateTime(snapshot.recentSuccess.completedAtMs),
                )}
              </p>
            )}
            {error && <p className="text-xs text-[var(--qp-danger)]">{error}</p>}
          </div>
        )}
      </QuietDialog>
    </>
  );
}

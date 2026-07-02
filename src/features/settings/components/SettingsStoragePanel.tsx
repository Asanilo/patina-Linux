import {
  AlertCircle,
  Database,
  FolderOpen,
  HardDrive,
  Move,
  RefreshCw,
  RotateCcw,
} from "lucide-react";
import type { ReactNode } from "react";
import QuietActionRow from "../../../shared/components/QuietActionRow";
import QuietSubpanel from "../../../shared/components/QuietSubpanel";
import QuietSwitch from "../../../shared/components/QuietSwitch";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import type { StorageSettingsState } from "../hooks/useStorageSettingsState.ts";
import { formatStorageBytes } from "../services/storagePathDisplay.ts";

interface SettingsStoragePanelProps {
  storage: StorageSettingsState;
}

interface StorageLocationRowProps {
  title: string;
  hint: string;
  path: string;
  sizeBytes: number;
  custom: boolean;
  busy: boolean;
  openLabel: string;
  moveLabel: string;
  restoreLabel: string;
  onOpen: () => void;
  onMove: () => void;
  onRestore: () => void;
  icon: ReactNode;
}

const actionButtonClass = "qp-button-secondary inline-flex h-8 shrink-0 items-center gap-1.5 px-2.5 text-xs font-semibold text-[var(--qp-text-secondary)] disabled:cursor-not-allowed disabled:opacity-50";

function StorageLocationRow({
  title,
  hint,
  path,
  sizeBytes,
  custom,
  busy,
  openLabel,
  moveLabel,
  restoreLabel,
  onOpen,
  onMove,
  onRestore,
  icon,
}: StorageLocationRowProps) {
  return (
    <QuietActionRow>
      <div className="flex min-w-0 flex-col gap-3 xl:flex-row xl:items-center xl:justify-between">
        <div className="min-w-0">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-[var(--qp-text-tertiary)]">{icon}</span>
            <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{title}</p>
            <span className={`qp-status ${custom ? "" : "qp-status-ok"} px-2 py-0.5 text-[11px] font-semibold`}>
              {custom ? UI_TEXT.settings.storageCustom : UI_TEXT.settings.storageDefault}
            </span>
            <span className="text-xs text-[var(--qp-text-tertiary)]">
              {UI_TEXT.settings.storageSizeLabel(formatStorageBytes(sizeBytes))}
            </span>
          </div>
          <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-secondary)]">{hint}</p>
          <p className="mt-1 truncate font-mono text-xs text-[var(--qp-text-tertiary)]" title={path}>{path}</p>
        </div>
        <div className="flex shrink-0 flex-wrap items-center gap-2 xl:justify-end">
          <button type="button" className={actionButtonClass} onClick={onOpen} disabled={busy} aria-label={openLabel} title={openLabel}>
            <FolderOpen size={14} />
            {UI_TEXT.settings.storageOpen}
          </button>
          <button type="button" className={actionButtonClass} onClick={onMove} disabled={busy} aria-label={moveLabel} title={moveLabel}>
            <Move size={14} />
            {UI_TEXT.settings.storageMove}
          </button>
          {custom ? (
            <button type="button" className={actionButtonClass} onClick={onRestore} disabled={busy} aria-label={restoreLabel} title={restoreLabel}>
              <RotateCcw size={14} />
              {UI_TEXT.settings.storageRestoreDefault}
            </button>
          ) : null}
        </div>
      </div>
    </QuietActionRow>
  );
}

export default function SettingsStoragePanel({ storage }: SettingsStoragePanelProps) {
  const { snapshot, loading, busyAction, error } = storage;
  const busy = busyAction !== null;

  if (loading) {
    return (
      <QuietSubpanel>
        <div className="flex items-center gap-2 text-sm text-[var(--qp-text-secondary)]">
          <RefreshCw size={14} className="animate-spin" />
          {UI_TEXT.settings.storageLoading}
        </div>
      </QuietSubpanel>
    );
  }

  if (!snapshot) {
    return (
      <QuietSubpanel tone="danger" className="flex flex-wrap items-center justify-between gap-3">
        <div className="flex min-w-0 items-center gap-2 text-sm text-[var(--qp-danger)]">
          <AlertCircle size={15} className="shrink-0" />
          <span>{error ?? UI_TEXT.settings.storageLoadFailed}</span>
        </div>
        <button type="button" className={actionButtonClass} onClick={() => void storage.refresh()} disabled={busy}>
          <RefreshCw size={14} />
          {UI_TEXT.settings.storageRetry}
        </button>
      </QuietSubpanel>
    );
  }

  const pending = snapshot.pendingMigration;
  const pendingSource = pending?.sourceDataRoot !== pending?.targetDataRoot
    ? pending?.sourceDataRoot
    : pending?.sourceWebviewRoot;
  const pendingTarget = pending?.sourceDataRoot !== pending?.targetDataRoot
    ? pending?.targetDataRoot
    : pending?.targetWebviewRoot;
  const retained = [
    snapshot.maintenance.retainedPreviousDataRoot
      ? { key: "data", label: UI_TEXT.settings.storageRetainedData, path: snapshot.maintenance.retainedPreviousDataRoot, kind: "retainedData" as const }
      : null,
    snapshot.maintenance.retainedPreviousWebviewRoot
      ? { key: "webview", label: UI_TEXT.settings.storageRetainedWebview, path: snapshot.maintenance.retainedPreviousWebviewRoot, kind: "retainedWebview" as const }
      : null,
  ].filter((entry): entry is NonNullable<typeof entry> => entry !== null);

  return (
    <QuietSubpanel>
      <div>
        <div className="flex items-center gap-2">
          <HardDrive size={15} className="text-[var(--qp-accent-default)]" />
          <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.storageLocalTitle}</p>
        </div>
        <p className="mt-1 text-sm leading-relaxed text-[var(--qp-text-secondary)]">{UI_TEXT.settings.storageLocalHint}</p>
      </div>

      <div className="mt-4 grid gap-3">
        <StorageLocationRow
          title={UI_TEXT.settings.storageDataTitle}
          hint={UI_TEXT.settings.storageDataHint}
          path={snapshot.paths.dataRoot}
          sizeBytes={snapshot.sizes.dataBytes}
          custom={snapshot.paths.isCustomDataRoot}
          busy={busy}
          openLabel={UI_TEXT.settings.storageOpenDataLabel}
          moveLabel={UI_TEXT.settings.storageMoveDataLabel}
          restoreLabel={UI_TEXT.settings.storageRestoreDataLabel}
          onOpen={() => void storage.openDirectory("data")}
          onMove={() => void storage.move("data")}
          onRestore={() => void storage.restoreDefault("data")}
          icon={<Database size={14} />}
        />
        <StorageLocationRow
          title={UI_TEXT.settings.storageWebviewTitle}
          hint={UI_TEXT.settings.storageWebviewHint}
          path={snapshot.paths.webviewRoot}
          sizeBytes={snapshot.sizes.webviewProfileBytes}
          custom={snapshot.paths.isCustomWebviewRoot}
          busy={busy}
          openLabel={UI_TEXT.settings.storageOpenWebviewLabel}
          moveLabel={UI_TEXT.settings.storageMoveWebviewLabel}
          restoreLabel={UI_TEXT.settings.storageRestoreWebviewLabel}
          onOpen={() => void storage.openDirectory("webview")}
          onMove={() => void storage.move("webview")}
          onRestore={() => void storage.restoreDefault("webview")}
          icon={<HardDrive size={14} />}
        />
        <QuietActionRow>
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.storageCacheTitle}</p>
              <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-secondary)]">
                {UI_TEXT.settings.storageCacheHint} {UI_TEXT.settings.storageSizeLabel(formatStorageBytes(snapshot.webviewCache.sizeBytes))}
              </p>
              <p className="mt-1 truncate font-mono text-xs text-[var(--qp-text-tertiary)]" title={snapshot.webviewCache.path}>
                {snapshot.webviewCache.path}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              <span className="text-xs font-medium text-[var(--qp-text-secondary)]">{UI_TEXT.settings.storageCacheClearOnRestart}</span>
              <QuietSwitch
                checked={snapshot.webviewCache.clearOnRestart}
                disabled={busy}
                ariaLabel={UI_TEXT.settings.storageCacheToggleLabel}
                onChange={(checked) => void storage.setCacheClearOnRestart(checked)}
              />
            </div>
          </div>
        </QuietActionRow>
      </div>

      {pending && pendingSource && pendingTarget ? (
        <div className="mt-4 border-t border-[var(--qp-border-subtle)] pt-4">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-[var(--qp-warning)]">{UI_TEXT.settings.storagePendingTitle}</p>
              <p className="mt-1 truncate font-mono text-xs text-[var(--qp-text-tertiary)]" title={UI_TEXT.settings.storagePendingDetail(pendingSource, pendingTarget)}>
                {UI_TEXT.settings.storagePendingDetail(pendingSource, pendingTarget)}
              </p>
            </div>
            <div className="flex flex-wrap items-center gap-2">
              <button type="button" className={actionButtonClass} onClick={() => void storage.cancelPending()} disabled={busy}>
                {UI_TEXT.settings.storagePendingCancel}
              </button>
              <button type="button" className="qp-button-primary inline-flex h-8 items-center gap-1.5 px-2.5 text-xs font-semibold disabled:opacity-50" onClick={() => void storage.restart()} disabled={busy} aria-label={UI_TEXT.settings.storageRestartPendingLabel}>
                <RefreshCw size={14} />
                {UI_TEXT.settings.storageRestartPending}
              </button>
            </div>
          </div>
        </div>
      ) : null}

      {retained.length > 0 ? (
        <div className="mt-4 border-t border-[var(--qp-border-subtle)] pt-4">
          <p className="text-xs font-semibold text-[var(--qp-text-secondary)]">{UI_TEXT.settings.storageRetainedTitle}</p>
          <div className="mt-2 grid gap-2">
            {retained.map((entry) => (
              <div key={entry.key} className="flex min-w-0 flex-wrap items-center justify-between gap-2">
                <div className="min-w-0">
                  <p className="text-xs font-medium text-[var(--qp-text-secondary)]">{entry.label}</p>
                  <p className="truncate font-mono text-xs text-[var(--qp-text-tertiary)]" title={entry.path}>{entry.path}</p>
                </div>
                <button type="button" className={actionButtonClass} onClick={() => void storage.openDirectory(entry.kind)} disabled={busy}>
                  <FolderOpen size={14} />
                  {UI_TEXT.settings.storageOpen}
                </button>
              </div>
            ))}
          </div>
        </div>
      ) : null}

      {snapshot.maintenance.lastError ? (
        <div className="mt-4 flex items-start gap-2 border-t border-[var(--qp-border-subtle)] pt-4 text-xs text-[var(--qp-danger)]">
          <AlertCircle size={14} className="mt-0.5 shrink-0" />
          <div className="min-w-0">
            <p className="font-semibold">{UI_TEXT.settings.storageLastErrorTitle}</p>
            <p className="mt-1 break-words font-mono">{snapshot.maintenance.lastError}</p>
          </div>
        </div>
      ) : null}

      {error ? (
        <p className="mt-4 text-xs text-[var(--qp-danger)]">{error}</p>
      ) : null}
    </QuietSubpanel>
  );
}

import { lazy, Suspense, useState } from "react";
import { Database, FileArchive, RefreshCw, RotateCcw, Trash2 } from "lucide-react";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import QuietDangerAction from "../../../shared/components/QuietDangerAction";
import QuietSubpanel from "../../../shared/components/QuietSubpanel";
import QuietActionRow from "../../../shared/components/QuietActionRow";
import QuietSelect from "../../../shared/components/QuietSelect";
import QuietSegmentedFilter from "../../../shared/components/QuietSegmentedFilter";
import QuietDialog from "../../../shared/components/QuietDialog";
import type { CleanupRange } from "../types";
import type { BackupRestoreStrategy } from "../services/settingsRuntimeAdapterService.ts";
import type { RemoteBackupEntry, RemoteBackupState } from "../hooks/useRemoteBackupState.ts";
import SettingsRemoteBackupPanel from "./SettingsRemoteBackupPanel";
import SettingsStoragePanel from "./SettingsStoragePanel";
import type { StorageSettingsState } from "../hooks/useStorageSettingsState.ts";

const SettingsScheduledBackupPanel = lazy(() => import("./SettingsScheduledBackupPanel.tsx"));
const SettingsActivityImportPanel = lazy(() => import("./SettingsActivityImportPanel.tsx"));

type CleanupOption = { value: CleanupRange; label: string };

type SettingsDataSafetyPanelProps = {
  cleanupRange: CleanupRange;
  cleanupOptions: CleanupOption[];
  restoreStrategy: BackupRestoreStrategy;
  isCleaning: boolean;
  isExportingBackup: boolean;
  isRestoringBackup: boolean;
  onCleanupRangeChange: (value: CleanupRange) => void;
  onRestoreStrategyChange: (value: BackupRestoreStrategy) => void;
  onCleanup: () => void;
  onExportBackup: () => void;
  onPrepareRestoreBackup: () => Promise<boolean | void>;
  onRestoreBackup: (restoreStrategy: BackupRestoreStrategy) => void;
  onClearPendingRestoreBackup: () => void;
  remoteBackup: RemoteBackupState;
  storage: StorageSettingsState;
};

export default function SettingsDataSafetyPanel({
  cleanupRange,
  cleanupOptions,
  restoreStrategy,
  isCleaning,
  isExportingBackup,
  isRestoringBackup,
  onCleanupRangeChange,
  onRestoreStrategyChange,
  onCleanup,
  onExportBackup,
  onPrepareRestoreBackup,
  onRestoreBackup,
  onClearPendingRestoreBackup,
  remoteBackup,
  storage,
}: SettingsDataSafetyPanelProps) {
  const [strategyDialogOpen, setStrategyDialogOpen] = useState(false);
  const [restoreStrategySource, setRestoreStrategySource] = useState<"local" | "remote">("local");
  const [pendingRemoteRestoreEntry, setPendingRemoteRestoreEntry] = useState<RemoteBackupEntry | null>(null);
  const [backupTargetDialogOpen, setBackupTargetDialogOpen] = useState(false);
  const [restoreSourceDialogOpen, setRestoreSourceDialogOpen] = useState(false);
  const hasRemoteBackupTarget = Boolean(remoteBackup.config && remoteBackup.hasSecret);
  const restoreStrategyOptions: Array<{ value: BackupRestoreStrategy; label: string }> = [
    { value: "merge", label: UI_TEXT.settings.restoreStrategyOptions.merge },
    { value: "replace", label: UI_TEXT.settings.restoreStrategyOptions.replace },
  ];
  const busy = isExportingBackup
    || isRestoringBackup
    || remoteBackup.isUploading
    || remoteBackup.isListing
    || remoteBackup.isDownloading;

  const handleBackupAction = () => {
    if (hasRemoteBackupTarget) {
      setBackupTargetDialogOpen(true);
      return;
    }
    onExportBackup();
  };

  const handleRestoreAction = () => {
    if (hasRemoteBackupTarget) {
      setRestoreSourceDialogOpen(true);
      return;
    }
    void prepareLocalRestore();
  };

  const prepareLocalRestore = async () => {
    setRestoreStrategySource("local");
    setPendingRemoteRestoreEntry(null);
    const prepared = await onPrepareRestoreBackup();
    if (prepared) {
      setStrategyDialogOpen(true);
    }
  };

  const openRemoteRestoreList = async () => {
    setRestoreStrategySource("remote");
    setPendingRemoteRestoreEntry(null);
    await remoteBackup.openRestoreDialog();
  };

  const handleRemoteRestoreEntrySelected = (entry: RemoteBackupEntry) => {
    remoteBackup.closeRestoreDialog();
    setRestoreStrategySource("remote");
    setPendingRemoteRestoreEntry(entry);
    setStrategyDialogOpen(true);
  };

  const confirmRestoreStrategy = () => {
    setStrategyDialogOpen(false);
    if (restoreStrategySource === "remote") {
      if (pendingRemoteRestoreEntry) {
        void remoteBackup.restoreEntry(pendingRemoteRestoreEntry, restoreStrategy);
      }
      setPendingRemoteRestoreEntry(null);
      return;
    }
    onRestoreBackup(restoreStrategy);
  };

  const closeStrategyDialog = () => {
    setStrategyDialogOpen(false);
    setPendingRemoteRestoreEntry(null);
    onClearPendingRestoreBackup();
  };

  return (
    <>
      <section className="qp-panel p-5 md:p-6">
        <div className="mb-5 flex items-center gap-2.5 border-b border-[var(--qp-border-subtle)] pb-2">
          <Database size={16} className="text-[var(--qp-accent-default)]" />
          <h2 className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.dataSafetyTitle}</h2>
        </div>

        <div className="space-y-5">
          <SettingsStoragePanel storage={storage} />

          <QuietSubpanel>
            <div>
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.backupRestoreTitle}</p>
              <p className="mt-1 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
                {UI_TEXT.settings.backupRestoreHint}
              </p>
            </div>

            <div className="mt-4 grid grid-cols-1 gap-3 lg:grid-cols-2">
              <QuietActionRow>
                <div className="flex items-end justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5">
                      <FileArchive size={14} className="text-[var(--qp-text-tertiary)]" />
                      <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
                        {UI_TEXT.settings.backupExportTitle}
                      </p>
                    </div>
                    <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-tertiary)]">
                      {UI_TEXT.settings.backupExportHint}
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={handleBackupAction}
                    disabled={busy}
                    className="qp-button-secondary h-8 shrink-0 rounded-[8px] px-3 text-xs font-semibold text-[var(--qp-text-secondary)] disabled:opacity-50"
                  >
                    {isExportingBackup || remoteBackup.isUploading ? UI_TEXT.settings.backupExporting : UI_TEXT.settings.backupExportAction}
                  </button>
                </div>
              </QuietActionRow>

              <QuietActionRow>
                <div className="flex items-end justify-between gap-3">
                  <div className="min-w-0">
                    <div className="flex items-center gap-1.5">
                      <RotateCcw size={14} className="text-[var(--qp-text-tertiary)]" />
                      <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
                        {UI_TEXT.settings.backupRestoreActionTitle}
                      </p>
                    </div>
                    <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-tertiary)]">
                      {UI_TEXT.settings.backupRestoreActionHint}
                    </p>
                  </div>
                  <button
                    type="button"
                    onClick={handleRestoreAction}
                    disabled={busy}
                    className="qp-button-secondary h-8 shrink-0 rounded-[8px] px-3 text-xs font-semibold text-[var(--qp-text-secondary)] disabled:opacity-50"
                  >
                    {isRestoringBackup || remoteBackup.isListing || remoteBackup.isDownloading ? UI_TEXT.settings.backupRestoring : UI_TEXT.settings.backupRestoreAction}
                  </button>
                </div>
              </QuietActionRow>
            </div>

            <Suspense fallback={<div className="qp-action-row mt-3 h-[72px]" aria-hidden="true" />}>
              <SettingsScheduledBackupPanel />
            </Suspense>

            <SettingsRemoteBackupPanel
              remoteBackup={remoteBackup}
              onRestoreEntrySelected={handleRemoteRestoreEntrySelected}
            />
          </QuietSubpanel>

          <Suspense fallback={<div className="qp-action-row h-[92px]" aria-hidden="true" />}>
            <SettingsActivityImportPanel />
          </Suspense>

          <QuietSubpanel tone="danger" className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
            <div className="min-w-0">
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.cleanupTitle}</p>
              <p className="mt-1 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
                {UI_TEXT.settings.cleanupHint}
              </p>
            </div>

            <div className="flex shrink-0 flex-wrap items-center gap-3 md:justify-end">
              <QuietSelect
                value={cleanupRange}
                onChange={(value) => onCleanupRangeChange(value as CleanupRange)}
                className="w-[128px]"
                options={cleanupOptions}
              />

              <QuietDangerAction
                onClick={onCleanup}
                disabled={isCleaning}
                leadingIcon={isCleaning ? <RefreshCw size={14} className="animate-spin" /> : <Trash2 size={14} />}
              >
                {isCleaning ? UI_TEXT.settings.cleanupRunning : UI_TEXT.settings.cleanupNow}
              </QuietDangerAction>
            </div>
          </QuietSubpanel>
        </div>
      </section>

      <QuietDialog
        open={backupTargetDialogOpen}
        title={UI_TEXT.settings.backupTargetTitle}
        description={UI_TEXT.settings.backupTargetHint}
        onClose={() => setBackupTargetDialogOpen(false)}
        closeOnBackdrop={!busy}
        actions={(
          <button
            type="button"
            onClick={() => setBackupTargetDialogOpen(false)}
            disabled={busy}
            className="qp-button-secondary h-8 min-h-0 rounded-[8px] px-3 text-xs font-semibold leading-none disabled:opacity-50"
          >
            {UI_TEXT.common.cancel}
          </button>
        )}
      >
        <div className="grid gap-3 md:grid-cols-2">
          <QuietActionRow>
            <button
              type="button"
              onClick={() => {
                setBackupTargetDialogOpen(false);
                onExportBackup();
              }}
              disabled={busy}
              className="block w-full border-0 bg-transparent p-0 text-left disabled:opacity-50"
            >
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.backupTargetLocalTitle}</p>
              <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-tertiary)]">{UI_TEXT.settings.backupTargetLocalHint}</p>
            </button>
          </QuietActionRow>
          <QuietActionRow>
            <button
              type="button"
              onClick={() => {
                setBackupTargetDialogOpen(false);
                void remoteBackup.uploadBackup();
              }}
              disabled={busy}
              className="block w-full border-0 bg-transparent p-0 text-left disabled:opacity-50"
            >
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.backupTargetRemoteTitle}</p>
              <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-tertiary)]">{UI_TEXT.settings.backupTargetRemoteHint}</p>
            </button>
          </QuietActionRow>
        </div>
      </QuietDialog>

      <QuietDialog
        open={restoreSourceDialogOpen}
        title={UI_TEXT.settings.restoreSourceTitle}
        description={UI_TEXT.settings.restoreSourceHint}
        onClose={() => setRestoreSourceDialogOpen(false)}
        closeOnBackdrop={!busy}
        actions={(
          <button
            type="button"
            onClick={() => setRestoreSourceDialogOpen(false)}
            disabled={busy}
            className="qp-button-secondary h-8 min-h-0 rounded-[8px] px-3 text-xs font-semibold leading-none disabled:opacity-50"
          >
            {UI_TEXT.common.cancel}
          </button>
        )}
      >
        <div className="grid gap-3 md:grid-cols-2">
          <QuietActionRow>
            <button
              type="button"
              onClick={() => {
                setRestoreSourceDialogOpen(false);
                void prepareLocalRestore();
              }}
              disabled={busy}
              className="block w-full border-0 bg-transparent p-0 text-left disabled:opacity-50"
            >
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.restoreSourceLocalTitle}</p>
              <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-tertiary)]">{UI_TEXT.settings.restoreSourceLocalHint}</p>
            </button>
          </QuietActionRow>
          <QuietActionRow>
            <button
              type="button"
              onClick={() => {
                setRestoreSourceDialogOpen(false);
                void openRemoteRestoreList();
              }}
              disabled={busy}
              className="block w-full border-0 bg-transparent p-0 text-left disabled:opacity-50"
            >
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.restoreSourceRemoteTitle}</p>
              <p className="mt-1 text-xs leading-relaxed text-[var(--qp-text-tertiary)]">{UI_TEXT.settings.restoreSourceRemoteHint}</p>
            </button>
          </QuietActionRow>
        </div>
      </QuietDialog>

      <QuietDialog
        open={strategyDialogOpen}
        title={UI_TEXT.settings.restoreStrategyLabel}
        description={UI_TEXT.settings.restoreStrategyHint}
        onClose={closeStrategyDialog}
        closeOnBackdrop={!isRestoringBackup}
        actions={(
          <>
            <button
              type="button"
              onClick={closeStrategyDialog}
              disabled={isRestoringBackup}
              className="qp-button-secondary h-8 min-h-0 rounded-[8px] px-3 text-xs font-semibold leading-none disabled:opacity-50"
            >
              {UI_TEXT.common.cancel}
            </button>
            <button
              type="button"
              onClick={() => {
                confirmRestoreStrategy();
              }}
              disabled={busy}
              className="qp-button-primary h-8 min-h-0 rounded-[8px] px-3 text-xs font-semibold leading-none disabled:opacity-50"
            >
              {isRestoringBackup || remoteBackup.isListing ? UI_TEXT.settings.backupRestoring : UI_TEXT.settings.backupRestoreAction}
            </button>
          </>
        )}
      >
        <div className="flex flex-col gap-3">
          <QuietSegmentedFilter
            value={restoreStrategy}
            options={restoreStrategyOptions}
            onChange={onRestoreStrategyChange}
          />
        </div>
      </QuietDialog>
    </>
  );
}

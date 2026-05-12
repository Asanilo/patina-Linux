import { Database, RefreshCw, Trash2 } from "lucide-react";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import QuietDangerAction from "../../../shared/components/QuietDangerAction";
import QuietSubpanel from "../../../shared/components/QuietSubpanel";
import QuietActionRow from "../../../shared/components/QuietActionRow";
import QuietSelect from "../../../shared/components/QuietSelect";
import type { CleanupRange } from "../types";

type CleanupOption = { value: CleanupRange; label: string };

type SettingsDataSafetyPanelProps = {
  cleanupRange: CleanupRange;
  cleanupOptions: CleanupOption[];
  isCleaning: boolean;
  isExportingBackup: boolean;
  isRestoringBackup: boolean;
  onCleanupRangeChange: (value: CleanupRange) => void;
  onCleanup: () => void;
  onExportBackup: () => void;
  onRestoreBackup: () => void;
};

export default function SettingsDataSafetyPanel({
  cleanupRange,
  cleanupOptions,
  isCleaning,
  isExportingBackup,
  isRestoringBackup,
  onCleanupRangeChange,
  onCleanup,
  onExportBackup,
  onRestoreBackup,
}: SettingsDataSafetyPanelProps) {
  return (
    <section className="qp-panel p-5 md:p-6">
      <div className="mb-5 flex items-center gap-2.5 border-b border-[var(--qp-border-subtle)] pb-2">
        <Database size={16} className="text-[var(--qp-danger)]" />
        <h2 className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.dataSafetyTitle}</h2>
      </div>

      <div className="space-y-5">
        <QuietSubpanel>
          <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.backupRestoreTitle}</p>
          <p className="mt-1 text-sm text-[var(--qp-text-secondary)]">
            {UI_TEXT.settings.backupRestoreHint}
          </p>

          <div className="mt-4 grid grid-cols-1 gap-3 lg:grid-cols-2">
            <QuietActionRow className="flex items-center justify-between">
              <div>
                <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.backupExportTitle}</p>
                <p className="mt-0.5 text-xs text-[var(--qp-text-tertiary)]">{UI_TEXT.settings.backupExportHint}</p>
              </div>
              <button
                type="button"
                onClick={onExportBackup}
                disabled={isExportingBackup || isRestoringBackup}
                className="qp-button-secondary rounded-[8px] px-3 py-2 text-xs font-semibold text-[var(--qp-text-secondary)] disabled:opacity-50"
              >
                {isExportingBackup ? UI_TEXT.settings.backupExporting : UI_TEXT.settings.backupExportAction}
              </button>
            </QuietActionRow>

            <QuietActionRow className="flex items-center justify-between">
              <div>
                <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
                  {UI_TEXT.settings.backupRestoreActionTitle}
                </p>
                <p className="mt-0.5 text-xs text-[var(--qp-text-tertiary)]">
                  {UI_TEXT.settings.backupRestoreActionHint}
                </p>
              </div>
              <button
                type="button"
                onClick={onRestoreBackup}
                disabled={isExportingBackup || isRestoringBackup}
                className="qp-button-secondary rounded-[8px] px-3 py-2 text-xs font-semibold text-[var(--qp-text-secondary)] disabled:opacity-50"
              >
                {isRestoringBackup ? UI_TEXT.settings.backupRestoring : UI_TEXT.settings.backupRestoreAction}
              </button>
            </QuietActionRow>
          </div>
        </QuietSubpanel>

        <QuietSubpanel tone="danger">
          <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.settings.cleanupTitle}</p>
          <p className="mt-1 text-sm text-[var(--qp-text-secondary)]">{UI_TEXT.settings.cleanupHint}</p>

          <div className="mt-3 flex flex-wrap items-center gap-3">
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
  );
}

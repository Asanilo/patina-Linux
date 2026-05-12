import { Info } from "lucide-react";
import QuietSubpanel from "../../../shared/components/QuietSubpanel";
import type { UpdateSnapshot } from "../../../shared/types/update";
import UpdateStatusPanel from "../../update/components/UpdateStatusPanel";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";

type AboutPanelProps = {
  appVersion: string;
  effectiveUpdateSnapshot: UpdateSnapshot;
  updateChecking: boolean;
  updateInstalling: boolean;
  updateDialogOpen: boolean;
  onCheckForUpdates?: () => void;
  onOpenUpdateDialog?: () => void;
  onOpenUpdateReleasePage?: () => void;
  onOpenUpdateDownload?: () => void;
  onOpenReleaseNotes: () => void;
  onOpenFeedback: () => void;
};

export default function AboutPanel({
  appVersion,
  effectiveUpdateSnapshot,
  updateChecking,
  updateInstalling,
  updateDialogOpen,
  onCheckForUpdates,
  onOpenUpdateDialog,
  onOpenUpdateReleasePage,
  onOpenUpdateDownload,
  onOpenReleaseNotes,
  onOpenFeedback,
}: AboutPanelProps) {
  return (
    <section className="qp-panel p-5 md:p-6">
      <div className="flex items-center gap-2.5 pb-2 border-b border-[var(--qp-border-subtle)] mb-5">
        <Info size={16} className="text-[var(--qp-accent-default)]" />
        <h2 className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.about.sectionTitle}</h2>
      </div>

      <QuietSubpanel>
        <p className="text-sm font-semibold text-[var(--qp-text-primary)]">{UI_TEXT.about.appInfo}</p>
        <p className="mt-1 text-sm text-[var(--qp-text-secondary)]">
          {UI_TEXT.about.currentVersion(appVersion)}
        </p>
        <p className="mt-0.5 text-xs text-[var(--qp-text-tertiary)]">
          {UI_TEXT.about.description}
        </p>
        <div className="mt-4">
          <UpdateStatusPanel
            snapshot={effectiveUpdateSnapshot}
            checking={updateChecking}
            installing={updateInstalling}
            suppressProgress={updateDialogOpen}
            onCheckUpdates={() => onCheckForUpdates?.()}
            onOpenConfirmDialog={() => onOpenUpdateDialog?.()}
            onOpenUpdateReleasePage={() => onOpenUpdateReleasePage?.()}
            onOpenUpdateDownload={() => onOpenUpdateDownload?.()}
            onOpenReleaseNotes={onOpenReleaseNotes}
            onOpenFeedback={onOpenFeedback}
          />
        </div>
      </QuietSubpanel>
    </section>
  );
}

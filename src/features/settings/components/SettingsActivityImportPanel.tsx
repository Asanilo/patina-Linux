import { FileInput, RefreshCw, Trash2 } from "lucide-react";
import { useCallback, useState } from "react";
import QuietActionRow from "../../../shared/components/QuietActionRow.tsx";
import QuietConfirmDialog from "../../../shared/components/QuietConfirmDialog.tsx";
import QuietDialog from "../../../shared/components/QuietDialog.tsx";
import QuietIconAction from "../../../shared/components/QuietIconAction.tsx";
import QuietSubpanel from "../../../shared/components/QuietSubpanel.tsx";
import { getUiTextLanguage, UI_TEXT } from "../../../shared/copy/uiText.ts";
import { getActivityImportCopy } from "../activityImportCopy.ts";
import {
  commitActivityImport,
  deleteActivityImportBatch,
  listActivityImportBatches,
  pickActivityImportFile,
  previewActivityImport,
  type ActivityImportBatch,
  type ActivityImportPreview,
} from "../services/settingsRuntimeAdapterService.ts";

export default function SettingsActivityImportPanel() {
  const copy = getActivityImportCopy(getUiTextLanguage());
  const [busy, setBusy] = useState(false);
  const [preview, setPreview] = useState<ActivityImportPreview | null>(null);
  const [batches, setBatches] = useState<ActivityImportBatch[] | null>(null);
  const [pendingDelete, setPendingDelete] = useState<ActivityImportBatch | null>(null);
  const [status, setStatus] = useState<{ text: string; error: boolean } | null>(null);

  const refreshBatches = useCallback(async () => {
    const next = await listActivityImportBatches();
    setBatches(next);
    return next;
  }, []);

  const chooseFile = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    setStatus(null);
    try {
      const filePath = await pickActivityImportFile();
      if (!filePath) return;
      setPreview(await previewActivityImport(filePath));
    } catch (error) {
      console.error("preview activity import failed", error);
      setStatus({ text: copy.failed, error: true });
    } finally {
      setBusy(false);
    }
  }, [busy, copy]);

  const commitPreview = useCallback(async () => {
    if (busy || !preview) return;
    setBusy(true);
    try {
      const report = await commitActivityImport(preview);
      setPreview(null);
      setStatus({ text: copy.success(report.importedRecords), error: false });
    } catch (error) {
      console.error("commit activity import failed", error);
      setStatus({ text: copy.failed, error: true });
    } finally {
      setBusy(false);
    }
  }, [busy, copy, preview]);

  const openBatches = useCallback(async () => {
    if (busy) return;
    setBusy(true);
    setStatus(null);
    try {
      await refreshBatches();
    } catch (error) {
      console.error("list activity import batches failed", error);
      setStatus({ text: copy.failed, error: true });
    } finally {
      setBusy(false);
    }
  }, [busy, copy, refreshBatches]);

  const confirmDelete = useCallback(async () => {
    if (busy || !pendingDelete) return;
    const batchId = pendingDelete.id;
    setPendingDelete(null);
    setBusy(true);
    try {
      await deleteActivityImportBatch(batchId);
      await refreshBatches();
    } catch (error) {
      console.error("delete activity import batch failed", error);
      setStatus({ text: copy.failed, error: true });
    } finally {
      setBusy(false);
    }
  }, [busy, copy, pendingDelete, refreshBatches]);

  const available = preview ? Math.max(0, preview.validRecords - preview.duplicateRecords) : 0;

  return (
    <>
      <QuietSubpanel>
        <div className="flex flex-col gap-4 md:flex-row md:items-center md:justify-between">
          <div className="min-w-0">
            <div className="flex items-center gap-1.5">
              <FileInput size={14} className="text-[var(--qp-text-tertiary)]" />
              <p className="text-sm font-semibold text-[var(--qp-text-primary)]">
                {copy.title}
              </p>
            </div>
            <p className="mt-1 text-sm leading-relaxed text-[var(--qp-text-secondary)]">
              {copy.hint}
            </p>
            {status ? (
              <p className={`mt-2 text-xs ${status.error ? "text-[var(--qp-danger)]" : "text-[var(--qp-success)]"}`}>
                {status.text}
              </p>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-2">
            <button
              type="button"
              className="qp-button-secondary h-8 rounded-[8px] px-3 text-xs font-semibold disabled:opacity-50"
              disabled={busy}
              onClick={() => void openBatches()}
            >
              {copy.manage}
            </button>
            <button
              type="button"
              className="qp-button-primary flex h-8 items-center gap-1.5 rounded-[8px] px-3 text-xs font-semibold disabled:opacity-50"
              disabled={busy}
              onClick={() => void chooseFile()}
            >
              {busy ? <RefreshCw size={13} className="animate-spin" /> : <FileInput size={13} />}
              {copy.action}
            </button>
          </div>
        </div>
      </QuietSubpanel>

      <QuietDialog
        open={preview !== null}
        title={copy.dialogTitle}
        description={copy.dialogHint}
        onClose={() => !busy && setPreview(null)}
        closeOnBackdrop={!busy}
        actions={(
          <>
            <button
              type="button"
              className="qp-button-secondary h-8 rounded-[8px] px-3 text-xs font-semibold"
              disabled={busy}
              onClick={() => setPreview(null)}
            >
              {UI_TEXT.common.cancel}
            </button>
            <button
              type="button"
              className="qp-button-primary h-8 rounded-[8px] px-3 text-xs font-semibold disabled:opacity-50"
              disabled={busy || available === 0}
              onClick={() => void commitPreview()}
            >
              {busy ? UI_TEXT.common.processing : copy.commit}
            </button>
          </>
        )}
      >
        {preview ? (
          <div className="space-y-3 text-sm text-[var(--qp-text-secondary)]">
            <p className="truncate font-semibold text-[var(--qp-text-primary)]" title={preview.filePath}>
              {preview.fileName}
            </p>
            <div className="grid grid-cols-1 gap-2 sm:grid-cols-2">
              <QuietActionRow>{copy.available(available)}</QuietActionRow>
              <QuietActionRow>
                {copy.breakdown(preview.exactSessions, preview.hourBuckets)}
              </QuietActionRow>
              {preview.duplicateRecords > 0 ? (
                <QuietActionRow>{copy.duplicates(preview.duplicateRecords)}</QuietActionRow>
              ) : null}
              {preview.errorRecords > 0 ? (
                <QuietActionRow>{copy.errors(preview.errorRecords)}</QuietActionRow>
              ) : null}
            </div>
            {preview.errors.length > 0 ? (
              <ul className="max-h-32 space-y-1 overflow-y-auto text-xs text-[var(--qp-danger)]">
                {preview.errors.map((error) => (
                  <li key={`${error.line}-${error.message}`}>L{error.line}: {error.message}</li>
                ))}
              </ul>
            ) : null}
          </div>
        ) : null}
      </QuietDialog>

      <QuietDialog
        open={batches !== null}
        title={copy.batchesTitle}
        description={copy.batchesHint}
        onClose={() => !busy && setBatches(null)}
        closeOnBackdrop={!busy}
        actions={(
          <button
            type="button"
            className="qp-button-secondary h-8 rounded-[8px] px-3 text-xs font-semibold"
            disabled={busy}
            onClick={() => setBatches(null)}
          >
            {UI_TEXT.common.close}
          </button>
        )}
      >
        <div className="space-y-2">
          {batches?.length ? batches.map((batch) => (
            <QuietActionRow key={batch.id}>
              <div className="flex min-w-0 items-center justify-between gap-3">
                <div className="min-w-0">
                  <p className="truncate text-sm font-semibold text-[var(--qp-text-primary)]" title={batch.sourceName}>
                    {batch.sourceName}
                  </p>
                  <p className="mt-1 text-xs text-[var(--qp-text-tertiary)]">
                    {new Date(batch.importedAt).toLocaleString()} · {batch.totalRecords}
                  </p>
                </div>
                <QuietIconAction
                  icon={<Trash2 size={14} />}
                  title={copy.delete}
                  tone="danger"
                  disabled={busy}
                  onClick={() => setPendingDelete(batch)}
                />
              </div>
            </QuietActionRow>
          )) : (
            <p className="py-6 text-center text-sm text-[var(--qp-text-tertiary)]">
              {copy.empty}
            </p>
          )}
        </div>
      </QuietDialog>

      <QuietConfirmDialog
        open={pendingDelete !== null}
        title={copy.delete}
        description={pendingDelete ? copy.deleteConfirm(pendingDelete.sourceName) : ""}
        confirmLabel={copy.delete}
        cancelLabel={UI_TEXT.common.cancel}
        danger
        confirmLoading={busy}
        onCancel={() => setPendingDelete(null)}
        onConfirm={() => void confirmDelete()}
      />
    </>
  );
}

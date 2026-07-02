import { useCallback, useEffect, useState } from "react";
import { UI_TEXT } from "../../../shared/copy/uiText.ts";
import type { QuietToastTone } from "../../../shared/components/QuietToast";
import { formatStorageBytes } from "../services/storagePathDisplay.ts";
import {
  restoreDefaultStorageWithDeps,
  scheduleStorageMoveWithDeps,
  StorageSettingsService,
} from "../services/storageSettingsActions.ts";
import type {
  StorageDirectoryKind,
  StorageMigrationPreview,
  StorageSnapshot,
  StorageTargetKind,
} from "../services/storageSettingsActions.ts";

interface ConfirmOptions {
  title: string;
  description?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  danger?: boolean;
}

export interface UseStorageSettingsStateOptions {
  confirm: (options: ConfirmOptions) => Promise<boolean>;
  notify: (message: string, tone?: QuietToastTone) => void;
}

export function useStorageSettingsState({
  confirm,
  notify,
}: UseStorageSettingsStateOptions) {
  const [snapshot, setSnapshot] = useState<StorageSnapshot | null>(null);
  const [loading, setLoading] = useState(true);
  const [busyAction, setBusyAction] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    const next = await StorageSettingsService.getSnapshot();
    setSnapshot(next);
    setError(null);
    return next;
  }, []);

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      try {
        const next = await StorageSettingsService.getSnapshot();
        if (!cancelled) {
          setSnapshot(next);
          setError(null);
        }
      } catch (loadError) {
        console.error("load storage settings failed", loadError);
        if (!cancelled) setError(UI_TEXT.settings.storageLoadFailed);
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const confirmPreview = useCallback((preview: StorageMigrationPreview, restoreDefault: boolean) => (
    confirm({
      title: restoreDefault
        ? UI_TEXT.settings.storageRestoreDefaultConfirmTitle
        : UI_TEXT.settings.storageMoveConfirmTitle,
      description: UI_TEXT.settings.storageMoveConfirmDetail(
        preview.currentDataRoot === preview.targetDataRoot
          ? preview.currentWebviewRoot
          : preview.currentDataRoot,
        preview.currentDataRoot === preview.targetDataRoot
          ? preview.targetWebviewRoot
          : preview.targetDataRoot,
        formatStorageBytes(preview.payloadSizeBytes),
      ),
      confirmLabel: UI_TEXT.settings.storageScheduleAction,
    })
  ), [confirm]);

  const offerRestart = useCallback(async () => {
    const restart = await confirm({
      title: UI_TEXT.settings.storageRestartTitle,
      description: UI_TEXT.settings.storageRestartDetail,
      confirmLabel: UI_TEXT.settings.storageRestartNow,
      cancelLabel: UI_TEXT.settings.storageRestartLater,
    });
    if (restart) await StorageSettingsService.restart();
  }, [confirm]);

  const runAction = useCallback(async <T,>(name: string, action: () => Promise<T>): Promise<T | null> => {
    if (busyAction) return null;
    setBusyAction(name);
    setError(null);
    try {
      return await action();
    } catch (actionError) {
      console.error(`storage action ${name} failed`, actionError);
      const message = actionError instanceof Error ? actionError.message : String(actionError);
      setError(message);
      notify(UI_TEXT.settings.storageActionFailed, "warning");
      return null;
    } finally {
      setBusyAction(null);
    }
  }, [busyAction, notify]);

  const move = useCallback(async (kind: StorageTargetKind) => {
    const initialPath = kind === "data" ? snapshot?.paths.dataRoot : snapshot?.paths.webviewRoot;
    const selectedParent = await StorageSettingsService.pickParent(initialPath);
    if (!selectedParent) return;
    const result = await runAction(`move-${kind}`, () => scheduleStorageMoveWithDeps(
      kind,
      selectedParent,
      {
        preview: StorageSettingsService.previewMove,
        confirm: (preview) => confirmPreview(preview, false),
        schedule: StorageSettingsService.scheduleMove,
      },
    ));
    if (!result || result.status !== "scheduled") return;
    await refresh();
    notify(UI_TEXT.settings.storageScheduled, "success");
    await offerRestart();
  }, [confirmPreview, notify, offerRestart, refresh, runAction, snapshot]);

  const restoreDefault = useCallback(async (kind: StorageTargetKind) => {
    const result = await runAction(`restore-${kind}`, () => restoreDefaultStorageWithDeps(kind, {
      preview: StorageSettingsService.previewRestoreDefault,
      confirm: (preview) => confirmPreview(preview, true),
      schedule: StorageSettingsService.scheduleRestoreDefault,
    }));
    if (!result || result.status !== "scheduled") return;
    await refresh();
    notify(UI_TEXT.settings.storageScheduled, "success");
    await offerRestart();
  }, [confirmPreview, notify, offerRestart, refresh, runAction]);

  const cancelPending = useCallback(async () => {
    const next = await runAction("cancel", StorageSettingsService.cancelPending);
    if (next) {
      setSnapshot(next);
      notify(UI_TEXT.settings.storagePendingCancelled, "success");
    }
  }, [notify, runAction]);

  const setCacheClearOnRestart = useCallback(async (pending: boolean) => {
    const next = await runAction("cache", () => StorageSettingsService.setCacheClearOnRestart(pending));
    if (next) setSnapshot(next);
  }, [runAction]);

  const openDirectory = useCallback(async (kind: StorageDirectoryKind) => {
    await runAction(`open-${kind}`, () => StorageSettingsService.openDirectory(kind));
  }, [runAction]);

  return {
    snapshot,
    loading,
    busyAction,
    error,
    refresh,
    move,
    restoreDefault,
    cancelPending,
    setCacheClearOnRestart,
    openDirectory,
    restart: StorageSettingsService.restart,
  };
}

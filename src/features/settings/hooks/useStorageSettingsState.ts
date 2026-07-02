import { useCallback, useEffect, useState } from "react";
import type { QuietToastTone } from "../../../shared/components/QuietToast";
import { formatStorageBytes } from "../services/storagePathDisplay.ts";
import {
  restoreDefaultStorageWithDeps,
  scheduleStorageMoveWithDeps,
  StorageSettingsService,
} from "../services/storageSettingsActions.ts";
import { getStorageSettingsCopy } from "../storageSettingsCopy.ts";
import type { AppLanguage } from "../../../shared/settings/appSettings.ts";
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
  language: AppLanguage;
}

export function useStorageSettingsState({
  confirm,
  notify,
  language,
}: UseStorageSettingsStateOptions) {
  const copy = getStorageSettingsCopy(language);
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
        if (!cancelled) setError(copy.storageLoadFailed);
      } finally {
        if (!cancelled) setLoading(false);
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [copy]);

  const confirmPreview = useCallback((preview: StorageMigrationPreview, restoreDefault: boolean) => (
    confirm({
      title: restoreDefault
        ? copy.storageRestoreDefaultConfirmTitle
        : copy.storageMoveConfirmTitle,
      description: copy.storageMoveConfirmDetail(
        preview.currentDataRoot === preview.targetDataRoot
          ? preview.currentWebviewRoot
          : preview.currentDataRoot,
        preview.currentDataRoot === preview.targetDataRoot
          ? preview.targetWebviewRoot
          : preview.targetDataRoot,
        formatStorageBytes(preview.payloadSizeBytes),
      ),
      confirmLabel: copy.storageScheduleAction,
    })
  ), [confirm, copy]);

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
      notify(copy.storageActionFailed, "warning");
      return null;
    } finally {
      setBusyAction(null);
    }
  }, [busyAction, copy, notify]);

  const offerRestart = useCallback(async () => {
    const restart = await confirm({
      title: copy.storageRestartTitle,
      description: copy.storageRestartDetail,
      confirmLabel: copy.storageRestartNow,
      cancelLabel: copy.storageRestartLater,
    });
    if (restart) await runAction("restart", StorageSettingsService.restart);
  }, [confirm, copy, runAction]);

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
    const refreshed = await runAction("refresh", refresh);
    if (!refreshed) return;
    notify(copy.storageScheduled, "success");
    await offerRestart();
  }, [confirmPreview, copy, notify, offerRestart, refresh, runAction, snapshot]);

  const restoreDefault = useCallback(async (kind: StorageTargetKind) => {
    const result = await runAction(`restore-${kind}`, () => restoreDefaultStorageWithDeps(kind, {
      preview: StorageSettingsService.previewRestoreDefault,
      confirm: (preview) => confirmPreview(preview, true),
      schedule: StorageSettingsService.scheduleRestoreDefault,
    }));
    if (!result || result.status !== "scheduled") return;
    const refreshed = await runAction("refresh", refresh);
    if (!refreshed) return;
    notify(copy.storageScheduled, "success");
    await offerRestart();
  }, [confirmPreview, copy, notify, offerRestart, refresh, runAction]);

  const cancelPending = useCallback(async () => {
    const next = await runAction("cancel", StorageSettingsService.cancelPending);
    if (next) {
      setSnapshot(next);
      notify(copy.storagePendingCancelled, "success");
    }
  }, [copy, notify, runAction]);

  const setCacheClearOnRestart = useCallback(async (pending: boolean) => {
    const next = await runAction("cache", () => StorageSettingsService.setCacheClearOnRestart(pending));
    if (next) setSnapshot(next);
  }, [runAction]);

  const openDirectory = useCallback(async (kind: StorageDirectoryKind) => {
    await runAction(`open-${kind}`, () => StorageSettingsService.openDirectory(kind));
  }, [runAction]);

  const reload = useCallback(async () => {
    await runAction("refresh", refresh);
  }, [refresh, runAction]);

  const restart = useCallback(async () => {
    await runAction("restart", StorageSettingsService.restart);
  }, [runAction]);

  return {
    snapshot,
    loading,
    busyAction,
    error,
    refresh: reload,
    move,
    restoreDefault,
    cancelPending,
    setCacheClearOnRestart,
    openDirectory,
    restart,
    copy,
  };
}

export type StorageSettingsState = ReturnType<typeof useStorageSettingsState>;

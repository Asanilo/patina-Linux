import { useCallback, useEffect, useRef, useState } from "react";
import { setUiTextLanguage, UI_TEXT } from "../../../shared/copy/uiText.ts";
import type { QuietToastTone } from "../../../shared/components/QuietToast";
import { useQuietDialogs } from "../../../shared/hooks/useQuietDialogs";
import { getSettingsBootstrapCache, setSettingsBootstrapCache } from "../services/settingsBootstrapCache";
import { loadSettingsPageBootstrap } from "../services/settingsBootstrapService.ts";
import { SettingsRuntimeAdapterService } from "../services/settingsRuntimeAdapterService";
import {
  commitPreparedBackupRestoreFlow,
  prepareBackupRestoreFlow,
  runBackupExportFlow,
  runSettingsCleanupFlow,
} from "../services/settingsPageActions.ts";
import {
  cancelSettingsPageState,
  saveSettingsPageStateWithDeps,
} from "./settingsPageStateInteractions.ts";
import type { AppSettings } from "../../../shared/settings/appSettings";
import type { ThemeLibrary } from "../../../shared/settings/colorSchemeOptions.ts";
import type { CleanupRange } from "../types";
import type {
  BackupRestorePreparation,
  BackupRestoreStrategy,
  LocalApiSettingsSnapshot,
} from "../services/settingsRuntimeAdapterService.ts";
import { useRemoteBackupState } from "./useRemoteBackupState.ts";
import { buildLocalApiConfigurationText } from "../services/settingsLocalApiService.ts";

const buildCleanupOptions = (): Array<{ value: CleanupRange; label: string }> => [
  { value: 180, label: UI_TEXT.settings.cleanupRangeLabels[180] },
  { value: 90, label: UI_TEXT.settings.cleanupRangeLabels[90] },
  { value: 60, label: UI_TEXT.settings.cleanupRangeLabels[60] },
  { value: 30, label: UI_TEXT.settings.cleanupRangeLabels[30] },
  { value: 15, label: UI_TEXT.settings.cleanupRangeLabels[15] },
  { value: 7, label: UI_TEXT.settings.cleanupRangeLabels[7] },
];

const IDLE_TIMEOUT_MINUTES_RANGE = { min: 5, max: 30 } as const;
const TIMELINE_MERGE_GAP_MINUTES_RANGE = { min: 1, max: 5 } as const;
const MIN_SESSION_MINUTES_RANGE = { min: 1, max: 10 } as const;

const clampMinute = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));
const secondsToMinute = (seconds: number, min: number, max: number) =>
  clampMinute(Math.round(seconds / 60), min, max);

export interface UseSettingsPageStateOptions {
  onSettingsChanged: (settings: AppSettings) => void;
  onColorSchemeSaved?: (settings: AppSettings) => void;
  onDirtyChange?: (dirty: boolean) => void;
  onToast?: (message: string, tone?: QuietToastTone) => void;
  onRegisterSaveHandler?: (handler: (() => Promise<boolean>) | null) => void;
}

export function useSettingsPageState({
  onSettingsChanged,
  onColorSchemeSaved,
  onDirtyChange,
  onToast,
  onRegisterSaveHandler,
}: UseSettingsPageStateOptions) {
  const { confirm, dialogs } = useQuietDialogs();
  const initialBootstrap = getSettingsBootstrapCache();
  const initialBootstrapRef = useRef(initialBootstrap);
  const [savedSettings, setSavedSettings] = useState<AppSettings | null>(
    () => (initialBootstrap ? { ...initialBootstrap.settings } : null),
  );
  const [draftSettings, setDraftSettings] = useState<AppSettings | null>(
    () => (initialBootstrap ? { ...initialBootstrap.settings } : null),
  );
  const [loading, setLoading] = useState(() => !initialBootstrap);
  const [saveStatus, setSaveStatus] = useState<"idle" | "saving" | "saved">("idle");
  const [localApiActionStatus, setLocalApiActionStatus] = useState<
    "idle" | "applying-port" | "rotating-token"
  >("idle");
  const [cleanupRange, setCleanupRange] = useState<CleanupRange>(30);
  const [isCleaning, setIsCleaning] = useState(false);
  const [exportPath, setExportPath] = useState("");
  const [restorePath, setRestorePath] = useState("");
  const [restoreStrategy, setRestoreStrategy] = useState<BackupRestoreStrategy>("merge");
  const [pendingRestorePreparation, setPendingRestorePreparation] = useState<BackupRestorePreparation | null>(null);
  const [isExportingBackup, setIsExportingBackup] = useState(false);
  const [isRestoringBackup, setIsRestoringBackup] = useState(false);
  const [appVersion, setAppVersion] = useState(() => initialBootstrap?.appVersion ?? "-");
  const hasUnsavedChangesRef = useRef(false);
  const cleanupOptions = buildCleanupOptions();

  const notify = useCallback((message: string, tone: QuietToastTone = "info") => {
    onToast?.(message, tone);
  }, [onToast]);

  const remoteBackup = useRemoteBackupState({
    confirm,
    notify,
    restoreBackup: SettingsRuntimeAdapterService.restoreBackup,
    reload: () => window.location.reload(),
  });

  useEffect(() => {
    let cancelled = false;
    const load = async () => {
      const hadCacheAtStart = Boolean(initialBootstrapRef.current);
      if (!hadCacheAtStart) {
        setLoading(true);
      }
      try {
        const bootstrap = await loadSettingsPageBootstrap();
        setSettingsBootstrapCache({
          settings: { ...bootstrap.settings },
          appVersion: bootstrap.appVersion,
        });
        if (cancelled) return;
        if (!hasUnsavedChangesRef.current) {
          setSavedSettings({ ...bootstrap.settings });
          setDraftSettings({ ...bootstrap.settings });
        }
        setAppVersion(bootstrap.appVersion);
      } catch (error) {
        console.error("load settings bootstrap failed", error);
      } finally {
        if (!cancelled && !hadCacheAtStart) {
          setLoading(false);
        }
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, []);

  const hasUnsavedChanges = (() => {
    if (!savedSettings || !draftSettings) {
      return false;
    }
    const keys = Object.keys(savedSettings) as Array<keyof AppSettings>;
    return keys.some((key) => savedSettings[key] !== draftSettings[key]);
  })();

  useEffect(() => {
    hasUnsavedChangesRef.current = hasUnsavedChanges;
  }, [hasUnsavedChanges]);

  useEffect(() => {
    onDirtyChange?.(hasUnsavedChanges);
  }, [hasUnsavedChanges, onDirtyChange]);

  useEffect(() => () => {
    onDirtyChange?.(false);
  }, [onDirtyChange]);

  const handleChange = useCallback(<K extends keyof AppSettings>(key: K, value: AppSettings[K]) => {
    setDraftSettings((current) => {
      if (!current) return current;
      return { ...current, [key]: value } as AppSettings;
    });
  }, []);

  const applyLocalApiSnapshot = useCallback((snapshot: LocalApiSettingsSnapshot) => {
    if (!savedSettings) return;
    const nextSavedSettings: AppSettings = {
      ...savedSettings,
      localApiPort: snapshot.port,
      localApiToken: snapshot.token,
    };
    setSavedSettings(nextSavedSettings);
    setDraftSettings((current) => current ? {
      ...current,
      localApiPort: snapshot.port,
      localApiToken: snapshot.token,
    } : current);
    const nextBootstrap = { settings: nextSavedSettings, appVersion };
    setSettingsBootstrapCache(nextBootstrap);
    onSettingsChanged(nextSavedSettings);
  }, [appVersion, onSettingsChanged, savedSettings]);

  const handleApplyLocalApiPort = useCallback(async (port: number): Promise<boolean> => {
    if (!savedSettings || localApiActionStatus !== "idle") return false;
    setLocalApiActionStatus("applying-port");
    try {
      const snapshot = await SettingsRuntimeAdapterService.applyLocalApiPort(port);
      applyLocalApiSnapshot(snapshot);
      notify(UI_TEXT.settings.localApiPortApplied, "success");
      return true;
    } catch (error) {
      console.error("apply local API port failed", error);
      notify(UI_TEXT.settings.localApiPortApplyFailed, "warning");
      return false;
    } finally {
      setLocalApiActionStatus("idle");
    }
  }, [applyLocalApiSnapshot, localApiActionStatus, notify, savedSettings]);

  const handleRotateLocalApiToken = useCallback(async (): Promise<boolean> => {
    if (!savedSettings || localApiActionStatus !== "idle") return false;
    const confirmed = await confirm({
      title: UI_TEXT.settings.localApiRotateTokenTitle,
      description: UI_TEXT.settings.localApiRotateTokenDetail,
      confirmLabel: UI_TEXT.dialog.confirm,
      danger: true,
    });
    if (!confirmed) return false;

    setLocalApiActionStatus("rotating-token");
    try {
      const snapshot = await SettingsRuntimeAdapterService.rotateLocalApiToken();
      applyLocalApiSnapshot(snapshot);
      try {
        await navigator.clipboard.writeText(buildLocalApiConfigurationText(snapshot));
        notify(UI_TEXT.settings.localApiTokenRotated, "success");
      } catch (error) {
        console.error("copy local API configuration failed", error);
        notify(UI_TEXT.settings.localApiTokenCopyFailed, "warning");
      }
      return true;
    } catch (error) {
      console.error("rotate local API token failed", error);
      notify(UI_TEXT.settings.saveFailed, "warning");
      return false;
    } finally {
      setLocalApiActionStatus("idle");
    }
  }, [applyLocalApiSnapshot, confirm, localApiActionStatus, notify, savedSettings]);

  const handleSave = useCallback(async (): Promise<boolean> => {
    if (!savedSettings || !draftSettings) return false;
    if (!hasUnsavedChanges) return true;
    if (saveStatus === "saving") return false;
    setSaveStatus("saving");
    try {
      const result = await saveSettingsPageStateWithDeps({
        savedSettings,
        draftSettings,
        appVersion,
        hasUnsavedChanges,
        saveStatus,
      }, {
        buildPatch: SettingsRuntimeAdapterService.buildSettingsPatch,
        commitPatch: SettingsRuntimeAdapterService.commitSettingsPatch,
      });
      if (result.nextSavedSettings) {
        setSavedSettings(result.nextSavedSettings);
      }
      if (result.nextDraftSettings) {
        setDraftSettings(result.nextDraftSettings);
      }
      if (result.nextBootstrap) {
        setSettingsBootstrapCache(result.nextBootstrap);
        setUiTextLanguage(result.nextBootstrap.settings.language);
        onSettingsChanged(result.nextBootstrap.settings);
      }
      setSaveStatus(result.nextSaveStatus);
      if (result.nextSaveStatus === "saved") {
        window.setTimeout(() => setSaveStatus("idle"), 1800);
      }
      if (result.toastKind === "runtime-sync-warning") {
        notify(UI_TEXT.toast.settingsRuntimeSyncPartial, "warning");
      } else {
        notify(UI_TEXT.settings.saved, "success");
      }
      return result.accepted;
    } catch (error) {
      console.error("save settings failed", error);
      setSaveStatus("idle");
      notify(UI_TEXT.settings.saveFailed, "warning");
      return false;
    }
  }, [appVersion, draftSettings, hasUnsavedChanges, notify, onSettingsChanged, saveStatus, savedSettings]);

  const handleSaveColorScheme = useCallback(async (library: ThemeLibrary): Promise<boolean> => {
    if (!savedSettings || !draftSettings) return false;
    if (saveStatus === "saving") return false;

    const key = library === "dark" ? "colorSchemeDark" : "colorSchemeLight";
    if (savedSettings[key] === draftSettings[key]) {
      return true;
    }

    setSaveStatus("saving");
    try {
      const nextSavedSettings = {
        ...savedSettings,
        [key]: draftSettings[key],
      };
      const result = await SettingsRuntimeAdapterService.commitSettingsPatch({
        [key]: draftSettings[key],
      });
      setSavedSettings(nextSavedSettings);
      setSettingsBootstrapCache({
        settings: nextSavedSettings,
        appVersion,
      });
      onColorSchemeSaved?.(nextSavedSettings);
      setSaveStatus("saved");
      window.setTimeout(() => setSaveStatus("idle"), 1800);
      if (result.runtimeSync === "failed") {
        notify(UI_TEXT.toast.settingsRuntimeSyncPartial, "warning");
      } else {
        notify(UI_TEXT.settings.saved, "success");
      }
      return true;
    } catch (error) {
      console.error("save color scheme failed", error);
      setSaveStatus("idle");
      notify(UI_TEXT.settings.saveFailed, "warning");
      return false;
    }
  }, [appVersion, draftSettings, notify, onColorSchemeSaved, saveStatus, savedSettings]);

  useEffect(() => {
    onRegisterSaveHandler?.(handleSave);
    return () => {
      onRegisterSaveHandler?.(null);
    };
  }, [handleSave, onRegisterSaveHandler]);

  const handleCancel = useCallback(() => {
    const result = cancelSettingsPageState({
      savedSettings,
      hasUnsavedChanges,
    });
    if (!result.cancelled || !result.nextDraftSettings) return;
    setDraftSettings(result.nextDraftSettings);
    setSaveStatus(result.nextSaveStatus);
    if (result.toastKind === "cancelled") {
      notify(UI_TEXT.settings.cancelled, "info");
    }
  }, [hasUnsavedChanges, notify, savedSettings]);

  const handleCleanup = useCallback(async () => {
    const selectedLabel = cleanupOptions.find((option) => option.value === cleanupRange)?.label
      ?? UI_TEXT.settings.confirmRangeFallback;
    await runSettingsCleanupFlow({
      cleanupRange,
      cleanupRangeLabel: selectedLabel,
      confirm,
      clearSessionsByRange: SettingsRuntimeAdapterService.clearSessionsByRange,
      notify,
      reload: () => window.location.reload(),
      onExecutionStart: () => setIsCleaning(true),
      onExecutionEnd: () => setIsCleaning(false),
      reportError: (message, error) => {
        console.error(message, error);
      },
    });
  }, [cleanupOptions, cleanupRange, confirm, notify]);

  const handleExportBackup = useCallback(async () => {
    if (isExportingBackup) return;
    await runBackupExportFlow({
      initialPath: exportPath,
      exportBackupWithPicker: SettingsRuntimeAdapterService.exportBackupWithPicker,
      setExportPath,
      notify,
      onExecutionStart: () => setIsExportingBackup(true),
      onExecutionEnd: () => setIsExportingBackup(false),
      reportError: (message, error) => {
        console.error(message, error);
      },
    });
  }, [exportPath, isExportingBackup, notify]);

  const handlePrepareRestoreBackup = useCallback(async () => {
    if (isRestoringBackup) return;
    const preparation = await prepareBackupRestoreFlow({
      initialPath: restorePath,
      prepareBackupRestore: SettingsRuntimeAdapterService.prepareBackupRestore,
      setRestorePath,
      notify,
      onExecutionStart: () => setIsRestoringBackup(true),
      onExecutionEnd: () => setIsRestoringBackup(false),
      reportError: (message, error) => {
        console.error(message, error);
      },
    });
    setPendingRestorePreparation(preparation);
    return Boolean(preparation);
  }, [isRestoringBackup, notify, restorePath]);

  const handleRestoreBackup = useCallback(async (selectedRestoreStrategy: BackupRestoreStrategy = restoreStrategy) => {
    if (isRestoringBackup || !pendingRestorePreparation) return;
    await commitPreparedBackupRestoreFlow({
      preparation: pendingRestorePreparation,
      restoreStrategy: selectedRestoreStrategy,
      confirm,
      restoreBackup: SettingsRuntimeAdapterService.restoreBackup,
      notify,
      reload: () => window.location.reload(),
      onExecutionStart: () => setIsRestoringBackup(true),
      onExecutionEnd: () => setIsRestoringBackup(false),
      reportError: (message, error) => {
        console.error(message, error);
      },
    });
    setPendingRestorePreparation(null);
  }, [confirm, isRestoringBackup, notify, pendingRestorePreparation, restoreStrategy]);

  const clearPendingRestoreBackup = useCallback(() => {
    setPendingRestorePreparation(null);
  }, []);

  const handleOpenReleaseNotes = useCallback(async () => {
    try {
      await SettingsRuntimeAdapterService.openReleaseNotes();
    } catch (error) {
      console.error("open release notes failed", error);
      notify(UI_TEXT.toast.releaseNotesOpenFailed, "warning");
    }
  }, [notify]);

  const handleOpenFeedback = useCallback(async () => {
    try {
      await SettingsRuntimeAdapterService.openFeedback();
    } catch (error) {
      console.error("open feedback link failed", error);
      notify(UI_TEXT.toast.feedbackOpenFailed, "warning");
    }
  }, [notify]);

  const idleTimeoutMinutes = draftSettings
    ? secondsToMinute(
      draftSettings.idleTimeoutSecs,
      IDLE_TIMEOUT_MINUTES_RANGE.min,
      IDLE_TIMEOUT_MINUTES_RANGE.max,
    )
    : IDLE_TIMEOUT_MINUTES_RANGE.min;
  const timelineMergeGapMinutes = draftSettings
    ? secondsToMinute(
      draftSettings.timelineMergeGapSecs,
      TIMELINE_MERGE_GAP_MINUTES_RANGE.min,
      TIMELINE_MERGE_GAP_MINUTES_RANGE.max,
    )
    : TIMELINE_MERGE_GAP_MINUTES_RANGE.min;
  const minSessionMinutes = draftSettings
    ? secondsToMinute(
      draftSettings.minSessionSecs,
      MIN_SESSION_MINUTES_RANGE.min,
      MIN_SESSION_MINUTES_RANGE.max,
    )
    : MIN_SESSION_MINUTES_RANGE.min;

  return {
    dialogs,
    loading,
    savedSettings,
    draftSettings,
    appVersion,
    saveStatus,
    hasUnsavedChanges,
    handleCancel,
    handleSave,
    handleSaveColorScheme,
    handleChange,
    localApiActionStatus,
    handleApplyLocalApiPort,
    handleRotateLocalApiToken,
    cleanupRange,
    setCleanupRange,
    restoreStrategy,
    setRestoreStrategy,
    isCleaning,
    isExportingBackup,
    isRestoringBackup,
    handleCleanup,
    handleExportBackup,
    handlePrepareRestoreBackup,
    handleRestoreBackup,
    clearPendingRestoreBackup,
    remoteBackup,
    handleOpenReleaseNotes,
    handleOpenFeedback,
    idleTimeoutMinutes,
    timelineMergeGapMinutes,
    minSessionMinutes,
    cleanupOptions,
    idleTimeoutMinutesRange: IDLE_TIMEOUT_MINUTES_RANGE,
    timelineMergeGapMinutesRange: TIMELINE_MERGE_GAP_MINUTES_RANGE,
    minSessionMinutesRange: MIN_SESSION_MINUTES_RANGE,
  };
}

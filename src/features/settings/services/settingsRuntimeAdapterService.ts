import {
  clearSessionsBefore,
  saveAppSettingsPatch,
  saveAppSetting,
  type AppSettings,
  type AppSettingsPatch,
} from "../../../platform/persistence/appSettingsStore.ts";
import {
  exportBackup,
  pickBackupFile,
  pickBackupSaveFile,
  previewBackup,
  restoreBackup,
  type BackupRestoreStrategy,
  type BackupPreview,
} from "../../../platform/backup/backupRuntimeGateway.ts";
import { openExternalUrl } from "../../../platform/desktop/externalUrlGateway.ts";
import { emitAppSettingsChanged } from "../../../platform/runtime/appSettingsEventGateway.ts";
import {
  setAfkThreshold,
  setAudioParticipationEnabled,
} from "../../../platform/runtime/trackingRuntimeGateway.ts";
import {
  applyLocalApiPort,
  rotateLocalApiToken,
} from "../../../platform/runtime/localApiDiagnosticsGateway.ts";
import { getUiLocale, UI_TEXT } from "../../../shared/copy/uiText.ts";
import type { CleanupRange } from "../types.ts";
import {
  buildSessionCleanupPlan,
  clearSessionsByRangeWithDeps,
} from "./sessionCleanupPolicy.ts";

export type { BackupPreview, BackupRestoreStrategy } from "../../../platform/backup/backupRuntimeGateway.ts";

export interface BackupRestorePreparation {
  path: string;
  preview: BackupPreview;
  previewSummary: string;
  compatible: boolean;
  incompatibilityMessage?: string;
}

type SettingsPatch = Partial<AppSettings>;
export interface SettingsCommitResult {
  persisted: boolean;
  runtimeSync: "synced" | "failed" | "not-needed";
  runtimeSyncErrors: string[];
}

interface SettingsCommitDeps {
  persistPatch: (patch: SettingsPatch) => Promise<void>;
  syncIdleTimeout: (seconds: number) => Promise<void>;
  syncAudioParticipation: (enabled: boolean) => Promise<void>;
  notifySettingsChanged: (patch: SettingsPatch) => Promise<void>;
}
type ExportBackupDeps = {
  pickBackupSaveFile: (initialPath?: string) => Promise<string | null>;
  exportBackup: (path: string) => Promise<string>;
};
type PrepareBackupRestoreDeps = {
  pickBackupFile: (initialPath?: string) => Promise<string | null>;
  previewBackup: (path: string) => Promise<BackupPreview>;
};

const RELEASE_NOTES_URL = "https://github.com/Asanilo/patina-Linux/releases";
const REPOSITORY_URL = "https://github.com/Asanilo/patina-Linux";
const FEEDBACK_URL = "https://github.com/Asanilo/patina-Linux/issues/new/choose";
const KOFI_SUPPORT_URL = "https://ko-fi.com/ceceliaee";

export function buildBackupPreviewSummary(preview: BackupPreview): string {
  const exportedAt = new Date(preview.exportedAtMs).toLocaleString(getUiLocale());
  return [
    `${UI_TEXT.backup.versionLabel(preview.version)}（${UI_TEXT.backup.schemaLabel(preview.schemaVersion)}）`,
    UI_TEXT.backup.exportedAt(exportedAt),
    UI_TEXT.backup.appVersion(preview.appVersion),
    UI_TEXT.backup.restoreSafety(preview.restoreMessage),
    UI_TEXT.backup.itemCounts(preview.sessionCount, preview.settingCount, preview.iconCacheCount),
  ].join("\n");
}

const exportBackupDeps: ExportBackupDeps = {
  pickBackupSaveFile,
  exportBackup,
};

const prepareBackupRestoreDeps: PrepareBackupRestoreDeps = {
  pickBackupFile,
  previewBackup,
};

const defaultSettingsCommitDeps: SettingsCommitDeps = {
  persistPatch: saveAppSettingsPatch,
  syncIdleTimeout: setAfkThreshold,
  syncAudioParticipation: setAudioParticipationEnabled,
  notifySettingsChanged: emitAppSettingsChanged,
};

export async function exportBackupWithPickerWithDeps(
  initialPath: string | undefined,
  deps: ExportBackupDeps,
): Promise<string | null> {
  const selectedPath = await deps.pickBackupSaveFile(initialPath);
  if (!selectedPath) {
    return null;
  }

  return deps.exportBackup(selectedPath);
}

export async function prepareBackupRestoreWithDeps(
  initialPath: string | undefined,
  deps: PrepareBackupRestoreDeps,
): Promise<BackupRestorePreparation | null> {
  const selectedPath = await deps.pickBackupFile(initialPath);
  if (!selectedPath) {
    return null;
  }

  const preview = await deps.previewBackup(selectedPath);
  if (!preview.restoreSupported) {
    return {
      path: selectedPath,
      preview,
      previewSummary: "",
      compatible: false,
      incompatibilityMessage: preview.restoreMessage,
    };
  }

  return {
    path: selectedPath,
    preview,
    previewSummary: buildBackupPreviewSummary(preview),
    compatible: true,
  };
}

export class SettingsRuntimeAdapterService {
  static async updateSetting<K extends keyof AppSettings>(key: K, value: AppSettings[K]) {
    await saveAppSetting(key, value);

    if (key === "idleTimeoutSecs") {
      await setAfkThreshold(value as number);
    }
  }

  static async clearSessionsByRange(range: CleanupRange, nowMs: number = Date.now()): Promise<void> {
    const cleanupPlan = buildSessionCleanupPlan(range, nowMs);
    await clearSessionsByRangeWithDeps(cleanupPlan.range, cleanupPlan.nowMs, {
      clearSessionsBefore,
    });
  }

  static async exportBackupWithPicker(initialPath?: string): Promise<string | null> {
    return exportBackupWithPickerWithDeps(initialPath, exportBackupDeps);
  }

  static async prepareBackupRestore(initialPath?: string): Promise<BackupRestorePreparation | null> {
    return prepareBackupRestoreWithDeps(initialPath, prepareBackupRestoreDeps);
  }

  static async restoreBackup(path: string, restoreStrategy: BackupRestoreStrategy): Promise<void> {
    await restoreBackup(path, restoreStrategy);
  }

  static async openReleaseNotes(): Promise<void> {
    await openExternalUrl(RELEASE_NOTES_URL);
  }

  static async openRepository(): Promise<void> {
    await openExternalUrl(REPOSITORY_URL);
  }

  static async openFeedback(): Promise<void> {
    await openExternalUrl(FEEDBACK_URL);
  }

  static async openKofiSupport(): Promise<void> {
    await openExternalUrl(KOFI_SUPPORT_URL);
  }

  static buildSettingsPatch(
    saved: AppSettings,
    draft: AppSettings,
  ): SettingsPatch {
    const patch: SettingsPatch = {};
    const patchRecord = patch as Record<keyof AppSettings, AppSettings[keyof AppSettings]>;
    const keys = Object.keys(saved) as Array<keyof AppSettings>;
    for (const key of keys) {
      if (key === "localApiPort" || key === "localApiToken") {
        continue;
      }
      if (saved[key] !== draft[key]) {
        patchRecord[key] = draft[key];
      }
    }
    return patch;
  }

  static async commitSettingsPatch(patch: SettingsPatch): Promise<SettingsCommitResult> {
    return commitSettingsPatchWithDeps(patch, defaultSettingsCommitDeps);
  }

  static async applyLocalApiPort(port: number) {
    return applyLocalApiPort(port);
  }

  static async rotateLocalApiToken() {
    return rotateLocalApiToken();
  }
}

export async function commitSettingsPatchWithDeps(
  patch: SettingsPatch,
  deps: SettingsCommitDeps,
): Promise<SettingsCommitResult> {
  const entries = Object.entries(patch) as Array<[keyof AppSettings, AppSettings[keyof AppSettings]]>;
  if (entries.length === 0) {
    return {
      persisted: true,
      runtimeSync: "not-needed",
      runtimeSyncErrors: [],
    };
  }

  await deps.persistPatch(patch as AppSettingsPatch);

  const runtimeSyncErrors: string[] = [];
  try {
    await deps.notifySettingsChanged(patch);
  } catch (error) {
    runtimeSyncErrors.push(error instanceof Error ? error.message : String(error));
  }

  const idleTimeoutSecs = patch.idleTimeoutSecs;
  const needsRuntimeSync = typeof idleTimeoutSecs === "number";
  if (needsRuntimeSync) {
    try {
      await deps.syncIdleTimeout(idleTimeoutSecs);
    } catch (error) {
      runtimeSyncErrors.push(error instanceof Error ? error.message : String(error));
    }
  }

  const audioParticipationEnabled = patch.audioParticipationEnabled;
  const needsAudioRuntimeSync = typeof audioParticipationEnabled === "boolean";
  if (needsAudioRuntimeSync) {
    try {
      await deps.syncAudioParticipation(audioParticipationEnabled);
    } catch (error) {
      runtimeSyncErrors.push(error instanceof Error ? error.message : String(error));
    }
  }

  return {
    persisted: true,
    runtimeSync: runtimeSyncErrors.length > 0
        ? "failed"
        : needsRuntimeSync || needsAudioRuntimeSync
          ? "synced"
          : "not-needed",
    runtimeSyncErrors,
  };
}

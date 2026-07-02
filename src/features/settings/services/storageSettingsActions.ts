import {
  cancelPendingStorageMigration,
  getStorageSnapshot,
  openStorageDirectory,
  pickStorageParent,
  previewRestoreDefaultStorage,
  previewStorageMigration,
  restartForStorageMaintenance,
  scheduleRestoreDefaultStorage,
  scheduleStorageMigration,
  scheduleWebviewCacheClear,
} from "../../../platform/storage/storageRuntimeGateway.ts";
import type {
  StorageDirectoryKind,
  StorageMigrationPreview,
  StoragePendingMigration,
  StorageTargetKind,
} from "../../../platform/storage/storageRuntimeGateway.ts";

export type {
  StorageDirectoryKind,
  StorageMigrationPreview,
  StoragePendingMigration,
  StorageSnapshot,
  StorageTargetKind,
} from "../../../platform/storage/storageRuntimeGateway.ts";

export const StorageSettingsService = {
  getSnapshot: getStorageSnapshot,
  pickParent: pickStorageParent,
  previewMove: previewStorageMigration,
  previewRestoreDefault: previewRestoreDefaultStorage,
  scheduleMove: scheduleStorageMigration,
  scheduleRestoreDefault: scheduleRestoreDefaultStorage,
  cancelPending: cancelPendingStorageMigration,
  setCacheClearOnRestart: scheduleWebviewCacheClear,
  openDirectory: (kind: StorageDirectoryKind) => openStorageDirectory(kind),
  restart: restartForStorageMaintenance,
};

export type StorageScheduleResult =
  | { status: "cancelled" }
  | { status: "scheduled"; pending: StoragePendingMigration; preview: StorageMigrationPreview };

interface StorageScheduleDeps {
  preview: (kind: StorageTargetKind, selectedParent: string) => Promise<StorageMigrationPreview>;
  confirm: (preview: StorageMigrationPreview) => Promise<boolean>;
  schedule: (kind: StorageTargetKind, selectedParent: string) => Promise<StoragePendingMigration>;
}

interface StorageRestoreDeps {
  preview: (kind: StorageTargetKind) => Promise<StorageMigrationPreview>;
  confirm: (preview: StorageMigrationPreview) => Promise<boolean>;
  schedule: (kind: StorageTargetKind) => Promise<StoragePendingMigration>;
}

export async function scheduleStorageMoveWithDeps(
  kind: StorageTargetKind,
  selectedParent: string,
  deps: StorageScheduleDeps,
): Promise<StorageScheduleResult> {
  const preview = await deps.preview(kind, selectedParent);
  if (!await deps.confirm(preview)) return { status: "cancelled" };
  const pending = await deps.schedule(kind, selectedParent);
  return { status: "scheduled", pending, preview };
}

export async function restoreDefaultStorageWithDeps(
  kind: StorageTargetKind,
  deps: StorageRestoreDeps,
): Promise<StorageScheduleResult> {
  const preview = await deps.preview(kind);
  if (!await deps.confirm(preview)) return { status: "cancelled" };
  const pending = await deps.schedule(kind);
  return { status: "scheduled", pending, preview };
}

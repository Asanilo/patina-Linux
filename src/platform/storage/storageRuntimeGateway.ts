import { invoke } from "@tauri-apps/api/core";

export type StorageTargetKind = "data" | "webview";
export type StorageDirectoryKind = "data" | "backups" | "webview" | "retainedData" | "retainedWebview";

export interface StorageMigrationPreview {
  currentDataRoot: string;
  targetDataRoot: string;
  currentWebviewRoot: string;
  targetWebviewRoot: string;
  payloadSizeBytes: number;
  availableSpaceBytes: number;
  requiredSpaceBytes: number;
  requiresRestart: boolean;
}

export interface StoragePendingMigration {
  id: string;
  sourceDataRoot: string;
  targetDataRoot: string;
  sourceWebviewRoot: string;
  targetWebviewRoot: string;
  createdAtMs: number;
}

export interface StorageSnapshot {
  paths: {
    dataRoot: string;
    defaultDataRoot: string;
    databasePath: string;
    backupDir: string;
    webviewRoot: string;
    defaultWebviewRoot: string;
    isCustomDataRoot: boolean;
    isCustomWebviewRoot: boolean;
  };
  sizes: {
    dataBytes: number;
    webviewProfileBytes: number;
  };
  webviewCache: {
    path: string;
    sizeBytes: number;
    clearOnRestart: boolean;
  };
  maintenance: {
    lastWebviewCacheClearAtMs: number | null;
    lastError: string | null;
    lastMigrationStatus: string | null;
    retainedPreviousDataRoot: string | null;
    retainedPreviousWebviewRoot: string | null;
  };
  pendingMigration: StoragePendingMigration | null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object";
}

function isNullableString(value: unknown): value is string | null {
  return typeof value === "string" || value === null;
}

function isPendingMigration(value: unknown): value is StoragePendingMigration {
  if (!isRecord(value)) return false;
  return typeof value.id === "string"
    && typeof value.sourceDataRoot === "string"
    && typeof value.targetDataRoot === "string"
    && typeof value.sourceWebviewRoot === "string"
    && typeof value.targetWebviewRoot === "string"
    && typeof value.createdAtMs === "number";
}

function parseMigrationPreview(value: unknown): StorageMigrationPreview {
  if (!isRecord(value)
    || typeof value.currentDataRoot !== "string"
    || typeof value.targetDataRoot !== "string"
    || typeof value.currentWebviewRoot !== "string"
    || typeof value.targetWebviewRoot !== "string"
    || typeof value.payloadSizeBytes !== "number"
    || typeof value.availableSpaceBytes !== "number"
    || typeof value.requiredSpaceBytes !== "number"
    || typeof value.requiresRestart !== "boolean") {
    throw new Error("Invalid storage migration preview payload");
  }
  return value as unknown as StorageMigrationPreview;
}

function parsePendingMigration(value: unknown): StoragePendingMigration {
  if (!isPendingMigration(value)) {
    throw new Error("Invalid pending storage migration payload");
  }
  return value;
}

function parseStorageSnapshot(value: unknown): StorageSnapshot {
  if (!isRecord(value)
    || !isRecord(value.paths)
    || !isRecord(value.sizes)
    || !isRecord(value.webviewCache)
    || !isRecord(value.maintenance)) {
    throw new Error("Invalid storage snapshot payload");
  }
  const { paths, sizes, webviewCache, maintenance, pendingMigration } = value;
  const valid = typeof paths.dataRoot === "string"
    && typeof paths.defaultDataRoot === "string"
    && typeof paths.databasePath === "string"
    && typeof paths.backupDir === "string"
    && typeof paths.webviewRoot === "string"
    && typeof paths.defaultWebviewRoot === "string"
    && typeof paths.isCustomDataRoot === "boolean"
    && typeof paths.isCustomWebviewRoot === "boolean"
    && typeof sizes.dataBytes === "number"
    && typeof sizes.webviewProfileBytes === "number"
    && typeof webviewCache.path === "string"
    && typeof webviewCache.sizeBytes === "number"
    && typeof webviewCache.clearOnRestart === "boolean"
    && (typeof maintenance.lastWebviewCacheClearAtMs === "number" || maintenance.lastWebviewCacheClearAtMs === null)
    && isNullableString(maintenance.lastError)
    && isNullableString(maintenance.lastMigrationStatus)
    && isNullableString(maintenance.retainedPreviousDataRoot)
    && isNullableString(maintenance.retainedPreviousWebviewRoot)
    && (pendingMigration === null || isPendingMigration(pendingMigration));
  if (!valid) {
    throw new Error("Invalid storage snapshot payload");
  }
  return value as unknown as StorageSnapshot;
}

export async function getStorageSnapshot(): Promise<StorageSnapshot> {
  return parseStorageSnapshot(await invoke<unknown>("cmd_get_storage_snapshot"));
}

export async function pickStorageParent(initialPath?: string): Promise<string | null> {
  return invoke<string | null>("cmd_pick_storage_parent", { initialPath: initialPath ?? null });
}

export async function previewStorageMigration(
  kind: StorageTargetKind,
  selectedParent: string,
): Promise<StorageMigrationPreview> {
  return parseMigrationPreview(await invoke<unknown>("cmd_preview_storage_migration", {
    kind,
    selectedParent,
  }));
}

export async function previewRestoreDefaultStorage(
  kind: StorageTargetKind,
): Promise<StorageMigrationPreview> {
  return parseMigrationPreview(await invoke<unknown>("cmd_preview_restore_default_storage", { kind }));
}

export async function scheduleStorageMigration(
  kind: StorageTargetKind,
  selectedParent: string,
): Promise<StoragePendingMigration> {
  return parsePendingMigration(await invoke<unknown>("cmd_schedule_storage_migration", {
    kind,
    selectedParent,
  }));
}

export async function scheduleRestoreDefaultStorage(
  kind: StorageTargetKind,
): Promise<StoragePendingMigration> {
  return parsePendingMigration(await invoke<unknown>("cmd_schedule_restore_default_storage", { kind }));
}

export async function cancelPendingStorageMigration(): Promise<StorageSnapshot> {
  return parseStorageSnapshot(await invoke<unknown>("cmd_cancel_pending_storage_migration"));
}

export async function scheduleWebviewCacheClear(pending: boolean): Promise<StorageSnapshot> {
  return parseStorageSnapshot(await invoke<unknown>("cmd_schedule_webview_cache_clear", { pending }));
}

export async function openStorageDirectory(kind: StorageDirectoryKind): Promise<void> {
  await invoke("cmd_open_storage_directory", { kind });
}

export async function restartForStorageMaintenance(): Promise<void> {
  await invoke("cmd_restart_for_storage_maintenance");
}

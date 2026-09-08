import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type ScheduledBackupCadence = "daily" | "weekly";

export interface ScheduledBackupConfigInput {
  enabled: boolean;
  cadence: ScheduledBackupCadence;
  weekday: number | null;
  localTimeMinutes: number;
  targetDir: string;
}

export interface ScheduledBackupConfig extends ScheduledBackupConfigInput {
  targetGeneration: string;
  scheduleAnchorAtMs: number;
  updatedAtMs: number;
}

export interface ScheduledBackupRun {
  runKey: string;
  targetGeneration: string;
  logicalDate: string;
  logicalTimeMinutes: number;
  targetPath: string;
  status: string;
  fileState: string;
  attemptCount: number;
  retryAtMs: number | null;
  startedAtMs: number;
  completedAtMs: number | null;
  archiveSha256: string | null;
  sizeBytes: number | null;
  errorCode: string | null;
  errorMessage: string | null;
  cleanupWarning: string | null;
  updatedAtMs: number;
}

export interface ScheduledBackupSnapshot {
  config: ScheduledBackupConfig;
  nextExecutionAtMs: number | null;
  recentSuccess: ScheduledBackupRun | null;
  recentFailure: ScheduledBackupRun | null;
  activeRun: ScheduledBackupRun | null;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return Boolean(value) && typeof value === "object";
}

function isNullableNumber(value: unknown): value is number | null {
  return value === null || typeof value === "number";
}

function isNullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function parseConfig(value: unknown): ScheduledBackupConfig {
  if (!isRecord(value)
    || typeof value.enabled !== "boolean"
    || (value.cadence !== "daily" && value.cadence !== "weekly")
    || !isNullableNumber(value.weekday)
    || typeof value.localTimeMinutes !== "number"
    || typeof value.targetDir !== "string"
    || typeof value.targetGeneration !== "string"
    || typeof value.scheduleAnchorAtMs !== "number"
    || typeof value.updatedAtMs !== "number") {
    throw new Error("Received invalid scheduled backup configuration");
  }
  return value as unknown as ScheduledBackupConfig;
}

function parseRun(value: unknown): ScheduledBackupRun | null {
  if (value === null) return null;
  if (!isRecord(value)
    || typeof value.runKey !== "string"
    || typeof value.targetGeneration !== "string"
    || typeof value.logicalDate !== "string"
    || typeof value.logicalTimeMinutes !== "number"
    || typeof value.targetPath !== "string"
    || typeof value.status !== "string"
    || typeof value.fileState !== "string"
    || typeof value.attemptCount !== "number"
    || !isNullableNumber(value.retryAtMs)
    || typeof value.startedAtMs !== "number"
    || !isNullableNumber(value.completedAtMs)
    || !isNullableString(value.archiveSha256)
    || !isNullableNumber(value.sizeBytes)
    || !isNullableString(value.errorCode)
    || !isNullableString(value.errorMessage)
    || !isNullableString(value.cleanupWarning)
    || typeof value.updatedAtMs !== "number") {
    throw new Error("Received invalid scheduled backup run");
  }
  return value as unknown as ScheduledBackupRun;
}

export function parseScheduledBackupSnapshot(value: unknown): ScheduledBackupSnapshot {
  if (!isRecord(value) || !isNullableNumber(value.nextExecutionAtMs)) {
    throw new Error("Received invalid scheduled backup snapshot");
  }
  return {
    config: parseConfig(value.config),
    nextExecutionAtMs: value.nextExecutionAtMs,
    recentSuccess: parseRun(value.recentSuccess),
    recentFailure: parseRun(value.recentFailure),
    activeRun: parseRun(value.activeRun),
  };
}

export async function getScheduledBackupSnapshot(): Promise<ScheduledBackupSnapshot> {
  return parseScheduledBackupSnapshot(await invoke<unknown>("cmd_get_scheduled_backup_snapshot"));
}

export async function saveScheduledBackupConfig(
  input: ScheduledBackupConfigInput,
): Promise<ScheduledBackupSnapshot> {
  return parseScheduledBackupSnapshot(await invoke<unknown>("cmd_save_scheduled_backup_config", { input }));
}

export async function pickScheduledBackupDirectory(initialPath?: string): Promise<string | null> {
  return invoke<string | null>("cmd_pick_scheduled_backup_directory", {
    initialPath: initialPath ?? null,
  });
}

export function subscribeScheduledBackupChanges(
  handler: () => void,
): Promise<UnlistenFn> {
  return listen("scheduled-backup-changed", handler);
}

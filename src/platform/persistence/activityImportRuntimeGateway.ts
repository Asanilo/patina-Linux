import { invoke } from "@tauri-apps/api/core";

export interface ActivityImportError {
  line: number;
  message: string;
}

export interface ActivityImportPreview {
  filePath: string;
  fileName: string;
  fileFingerprint: string;
  validRecords: number;
  duplicateRecords: number;
  errorRecords: number;
  exactSessions: number;
  hourBuckets: number;
  errors: ActivityImportError[];
}

export interface ActivityImportReport {
  batchId: string | null;
  importedRecords: number;
  duplicateRecords: number;
  errorRecords: number;
  exactSessions: number;
  hourBuckets: number;
}

export interface ActivityImportBatch {
  id: string;
  importedAt: number;
  sourceName: string;
  sourceKind: string;
  exactSessions: number;
  hourBuckets: number;
  totalRecords: number;
}

export interface ActivityImportDeleteReport {
  deletedExactSessions: number;
  deletedHourBuckets: number;
}

function isPlainRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function hasFields(
  value: unknown,
  stringFields: readonly string[],
  numberFields: readonly string[],
): value is Record<string, unknown> {
  return isPlainRecord(value)
    && stringFields.every((field) => typeof value[field] === "string")
    && numberFields.every((field) => typeof value[field] === "number" && Number.isFinite(value[field]));
}

export function parseActivityImportPreview(value: unknown): ActivityImportPreview {
  if (!hasFields(
    value,
    ["filePath", "fileName", "fileFingerprint"],
    ["validRecords", "duplicateRecords", "errorRecords", "exactSessions", "hourBuckets"],
  ) || !Array.isArray(value.errors) || !value.errors.every((item) => hasFields(item, ["message"], ["line"]))) {
    throw new Error("Received invalid activity import preview");
  }
  return value as unknown as ActivityImportPreview;
}

export function parseActivityImportReport(value: unknown): ActivityImportReport {
  if (!hasFields(
    value,
    [],
    ["importedRecords", "duplicateRecords", "errorRecords", "exactSessions", "hourBuckets"],
  ) || !(typeof value.batchId === "string" || value.batchId === null)) {
    throw new Error("Received invalid activity import report");
  }
  return value as unknown as ActivityImportReport;
}

export function parseActivityImportBatches(value: unknown): ActivityImportBatch[] {
  if (!Array.isArray(value) || !value.every((item) => hasFields(
    item,
    ["id", "sourceName", "sourceKind"],
    ["importedAt", "exactSessions", "hourBuckets", "totalRecords"],
  ))) {
    throw new Error("Received invalid activity import batches");
  }
  return value as unknown as ActivityImportBatch[];
}

export function pickActivityImportFile(initialPath?: string): Promise<string | null> {
  return invoke("cmd_pick_activity_import_file", { initialPath: initialPath ?? null });
}

export async function previewActivityImport(filePath: string): Promise<ActivityImportPreview> {
  return parseActivityImportPreview(await invoke("cmd_preview_activity_import", { filePath }));
}

export async function commitActivityImport(preview: ActivityImportPreview): Promise<ActivityImportReport> {
  return parseActivityImportReport(await invoke("cmd_commit_activity_import", {
    filePath: preview.filePath,
    expectedFingerprint: preview.fileFingerprint,
  }));
}

export async function listActivityImportBatches(): Promise<ActivityImportBatch[]> {
  return parseActivityImportBatches(await invoke("cmd_list_activity_import_batches"));
}

export async function deleteActivityImportBatch(batchId: string): Promise<ActivityImportDeleteReport> {
  const value = await invoke("cmd_delete_activity_import_batch", { batchId });
  if (!hasFields(value, [], ["deletedExactSessions", "deletedHourBuckets"])) {
    throw new Error("Received invalid activity import deletion report");
  }
  return value as unknown as ActivityImportDeleteReport;
}

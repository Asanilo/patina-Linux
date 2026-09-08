import { invoke } from "@tauri-apps/api/core";

const GET_DAEMON_SERVICE_DIAGNOSTICS_COMMAND = "cmd_get_daemon_service_diagnostics";
const RETRY_RUNTIME_OWNER_CUTOVER_COMMAND = "cmd_retry_runtime_owner_cutover";
const SET_BACKGROUND_TRACKING_AT_LOGIN_COMMAND = "cmd_set_background_tracking_at_login";
const ROLLBACK_RUNTIME_OWNER_COMMAND = "cmd_rollback_runtime_owner_to_embedded";

export type DaemonServiceMigrationState =
  | "blocked"
  | "cutover-failed"
  | "cutover-pending"
  | "embedded-rollback"
  | "not-installed"
  | "owner-conflict"
  | "managed"
  | "managed-blocked"
  | "preference-mismatch"
  | "rollback-pending"
  | "ready"
  | "not-requested"
  | "unsupported";

export type RuntimeOwnerCutoverState =
  | "not-requested"
  | "prepared"
  | "activating"
  | "completed"
  | "failed"
  | "rolling-back"
  | "rolled-back"
  | "blocked"
  | "unsupported";

interface RawRuntimeOwnerCutoverDiagnosticsSnapshot {
  state: RuntimeOwnerCutoverState;
  request_id: string | null;
  updated_at_ms: number | null;
  failure_code: string | null;
  failure_message: string | null;
  background_tracking_at_login: boolean | null;
}

export interface RuntimeOwnerCutoverDiagnosticsSnapshot {
  state: RuntimeOwnerCutoverState;
  requestId: string | null;
  updatedAtMs: number | null;
  failureCode: string | null;
  failureMessage: string | null;
  backgroundTrackingAtLogin: boolean | null;
}

interface RawDaemonServiceDiagnosticsSnapshot {
  service_name: string;
  manager_available: boolean;
  unit_installed: boolean;
  unit_file_state: string | null;
  enabled: boolean;
  active_state: string | null;
  sub_state: string | null;
  active: boolean;
  migration_state: DaemonServiceMigrationState;
  migration_reason: string;
  control_available: boolean;
  error: string | null;
  cutover: RawRuntimeOwnerCutoverDiagnosticsSnapshot;
}

export interface DaemonServiceDiagnosticsSnapshot {
  serviceName: string;
  managerAvailable: boolean;
  unitInstalled: boolean;
  unitFileState: string | null;
  enabled: boolean;
  activeState: string | null;
  subState: string | null;
  active: boolean;
  migrationState: DaemonServiceMigrationState;
  migrationReason: string;
  controlAvailable: boolean;
  error: string | null;
  cutover: RuntimeOwnerCutoverDiagnosticsSnapshot;
}

const MIGRATION_STATES = new Set<DaemonServiceMigrationState>([
  "blocked",
  "cutover-failed",
  "cutover-pending",
  "embedded-rollback",
  "not-installed",
  "owner-conflict",
  "managed",
  "managed-blocked",
  "preference-mismatch",
  "rollback-pending",
  "ready",
  "not-requested",
  "unsupported",
]);

const CUTOVER_STATES = new Set<RuntimeOwnerCutoverState>([
  "not-requested",
  "prepared",
  "activating",
  "completed",
  "failed",
  "rolling-back",
  "rolled-back",
  "blocked",
  "unsupported",
]);

function isNullableString(value: unknown): value is string | null {
  return typeof value === "string" || value === null;
}

function isRawRuntimeOwnerCutoverDiagnostics(
  value: unknown,
): value is RawRuntimeOwnerCutoverDiagnosticsSnapshot {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;

  return typeof record.state === "string"
    && CUTOVER_STATES.has(record.state as RuntimeOwnerCutoverState)
    && isNullableString(record.request_id)
    && (record.updated_at_ms === null || typeof record.updated_at_ms === "number")
    && isNullableString(record.failure_code)
    && isNullableString(record.failure_message)
    && (record.background_tracking_at_login === null
      || typeof record.background_tracking_at_login === "boolean");
}

function isRawDaemonServiceDiagnostics(
  value: unknown,
): value is RawDaemonServiceDiagnosticsSnapshot {
  if (!value || typeof value !== "object") return false;
  const record = value as Record<string, unknown>;

  return typeof record.service_name === "string"
    && typeof record.manager_available === "boolean"
    && typeof record.unit_installed === "boolean"
    && isNullableString(record.unit_file_state)
    && typeof record.enabled === "boolean"
    && isNullableString(record.active_state)
    && isNullableString(record.sub_state)
    && typeof record.active === "boolean"
    && typeof record.migration_state === "string"
    && MIGRATION_STATES.has(record.migration_state as DaemonServiceMigrationState)
    && typeof record.migration_reason === "string"
    && typeof record.control_available === "boolean"
    && isNullableString(record.error)
    && isRawRuntimeOwnerCutoverDiagnostics(record.cutover);
}

function mapRawDaemonServiceDiagnostics(
  raw: RawDaemonServiceDiagnosticsSnapshot,
): DaemonServiceDiagnosticsSnapshot {
  return {
    serviceName: raw.service_name,
    managerAvailable: raw.manager_available,
    unitInstalled: raw.unit_installed,
    unitFileState: raw.unit_file_state,
    enabled: raw.enabled,
    activeState: raw.active_state,
    subState: raw.sub_state,
    active: raw.active,
    migrationState: raw.migration_state,
    migrationReason: raw.migration_reason,
    controlAvailable: raw.control_available,
    error: raw.error,
    cutover: {
      state: raw.cutover.state,
      requestId: raw.cutover.request_id,
      updatedAtMs: raw.cutover.updated_at_ms,
      failureCode: raw.cutover.failure_code,
      failureMessage: raw.cutover.failure_message,
      backgroundTrackingAtLogin: raw.cutover.background_tracking_at_login,
    },
  };
}

export async function getDaemonServiceDiagnostics(): Promise<DaemonServiceDiagnosticsSnapshot> {
  const payload = await invoke<unknown>(GET_DAEMON_SERVICE_DIAGNOSTICS_COMMAND);
  if (!isRawDaemonServiceDiagnostics(payload)) {
    throw new Error("Invalid daemon service diagnostics payload");
  }

  return mapRawDaemonServiceDiagnostics(payload);
}

export async function retryRuntimeOwnerCutover(): Promise<void> {
  await invoke(RETRY_RUNTIME_OWNER_CUTOVER_COMMAND, { confirmed: true });
}

export async function setBackgroundTrackingAtLogin(
  enabled: boolean,
): Promise<DaemonServiceDiagnosticsSnapshot> {
  const payload = await invoke<unknown>(SET_BACKGROUND_TRACKING_AT_LOGIN_COMMAND, { enabled });
  if (!isRawDaemonServiceDiagnostics(payload)) {
    throw new Error("Invalid daemon service diagnostics payload");
  }
  return mapRawDaemonServiceDiagnostics(payload);
}

export async function rollbackRuntimeOwnerToEmbedded(): Promise<void> {
  await invoke(ROLLBACK_RUNTIME_OWNER_COMMAND, { confirmed: true });
}

import { invoke } from "@tauri-apps/api/core";

const GET_DAEMON_SERVICE_DIAGNOSTICS_COMMAND = "cmd_get_daemon_service_diagnostics";

export type DaemonServiceMigrationState =
  | "blocked"
  | "not-installed"
  | "owner-conflict"
  | "managed"
  | "managed-blocked"
  | "ready"
  | "not-requested"
  | "unsupported";

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
}

const MIGRATION_STATES = new Set<DaemonServiceMigrationState>([
  "blocked",
  "not-installed",
  "owner-conflict",
  "managed",
  "managed-blocked",
  "ready",
  "not-requested",
  "unsupported",
]);

function isNullableString(value: unknown): value is string | null {
  return typeof value === "string" || value === null;
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
    && isNullableString(record.error);
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
  };
}

export async function getDaemonServiceDiagnostics(): Promise<DaemonServiceDiagnosticsSnapshot> {
  const payload = await invoke<unknown>(GET_DAEMON_SERVICE_DIAGNOSTICS_COMMAND);
  if (!isRawDaemonServiceDiagnostics(payload)) {
    throw new Error("Invalid daemon service diagnostics payload");
  }

  return mapRawDaemonServiceDiagnostics(payload);
}

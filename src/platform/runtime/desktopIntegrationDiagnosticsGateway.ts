import { invoke } from "@tauri-apps/api/core";

const GET_DESKTOP_INTEGRATION_DIAGNOSTICS_COMMAND = "cmd_get_desktop_integration_diagnostics";
const REPAIR_AUTOSTART_DESKTOP_FILE_COMMAND = "cmd_repair_autostart_desktop_file";

interface RawAutostartDiagnosticsSnapshot {
  path: string;
  exists: boolean;
  exec: string | null;
  valid: boolean;
  reason: string | null;
}

interface RawDesktopIntegrationDiagnosticsSnapshot {
  launch_at_login: boolean;
  start_minimized: boolean;
  autostart: RawAutostartDiagnosticsSnapshot;
}

export interface AutostartDiagnosticsSnapshot {
  path: string;
  exists: boolean;
  exec: string | null;
  valid: boolean;
  reason: string | null;
}

export interface DesktopIntegrationDiagnosticsSnapshot {
  launchAtLogin: boolean;
  startMinimized: boolean;
  autostart: AutostartDiagnosticsSnapshot;
}

function isRawAutostartDiagnostics(value: unknown): value is RawAutostartDiagnosticsSnapshot {
  if (!value || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return typeof record.path === "string"
    && typeof record.exists === "boolean"
    && (typeof record.exec === "string" || record.exec === null)
    && typeof record.valid === "boolean"
    && (typeof record.reason === "string" || record.reason === null);
}

function isRawDesktopIntegrationDiagnostics(
  value: unknown,
): value is RawDesktopIntegrationDiagnosticsSnapshot {
  if (!value || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return typeof record.launch_at_login === "boolean"
    && typeof record.start_minimized === "boolean"
    && isRawAutostartDiagnostics(record.autostart);
}

function mapRawDesktopIntegrationDiagnostics(
  raw: RawDesktopIntegrationDiagnosticsSnapshot,
): DesktopIntegrationDiagnosticsSnapshot {
  return {
    launchAtLogin: raw.launch_at_login,
    startMinimized: raw.start_minimized,
    autostart: raw.autostart,
  };
}

export async function getDesktopIntegrationDiagnostics(): Promise<DesktopIntegrationDiagnosticsSnapshot> {
  const payload = await invoke<unknown>(GET_DESKTOP_INTEGRATION_DIAGNOSTICS_COMMAND);
  if (!isRawDesktopIntegrationDiagnostics(payload)) {
    throw new Error("Invalid desktop integration diagnostics payload");
  }

  return mapRawDesktopIntegrationDiagnostics(payload);
}

export async function repairAutostartDesktopFile(): Promise<DesktopIntegrationDiagnosticsSnapshot> {
  const payload = await invoke<unknown>(REPAIR_AUTOSTART_DESKTOP_FILE_COMMAND);
  if (!isRawDesktopIntegrationDiagnostics(payload)) {
    throw new Error("Invalid desktop integration diagnostics payload");
  }

  return mapRawDesktopIntegrationDiagnostics(payload);
}

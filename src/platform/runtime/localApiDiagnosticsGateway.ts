import { invoke } from "@tauri-apps/api/core";

const GET_LOCAL_API_DIAGNOSTICS_COMMAND = "cmd_get_local_api_diagnostics";
const GET_LOCAL_API_SETTINGS_COMMAND = "cmd_get_local_api_settings";

interface RawLocalApiDiagnosticsSnapshot {
  base_url: string;
  token_path: string;
  token_present: boolean;
  listening: boolean;
}

interface RawLocalApiSettingsSnapshot {
  port: number;
  token: string;
  token_path: string;
  base_url: string;
}

export interface LocalApiDiagnosticsSnapshot {
  baseUrl: string;
  tokenPath: string;
  tokenPresent: boolean;
  listening: boolean;
}

export interface LocalApiSettingsSnapshot {
  port: number;
  token: string;
  tokenPath: string;
  baseUrl: string;
}

function isRawLocalApiDiagnostics(value: unknown): value is RawLocalApiDiagnosticsSnapshot {
  if (!value || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return typeof record.base_url === "string"
    && typeof record.token_path === "string"
    && typeof record.token_present === "boolean"
    && typeof record.listening === "boolean";
}

function isRawLocalApiSettings(value: unknown): value is RawLocalApiSettingsSnapshot {
  if (!value || typeof value !== "object") {
    return false;
  }

  const record = value as Record<string, unknown>;
  return typeof record.port === "number"
    && Number.isInteger(record.port)
    && typeof record.token === "string"
    && typeof record.token_path === "string"
    && typeof record.base_url === "string";
}

function mapRawLocalApiDiagnostics(raw: RawLocalApiDiagnosticsSnapshot): LocalApiDiagnosticsSnapshot {
  return {
    baseUrl: raw.base_url,
    tokenPath: raw.token_path,
    tokenPresent: raw.token_present,
    listening: raw.listening,
  };
}

function mapRawLocalApiSettings(raw: RawLocalApiSettingsSnapshot): LocalApiSettingsSnapshot {
  return {
    port: raw.port,
    token: raw.token,
    tokenPath: raw.token_path,
    baseUrl: raw.base_url,
  };
}

export async function getLocalApiDiagnostics(): Promise<LocalApiDiagnosticsSnapshot> {
  const payload = await invoke<unknown>(GET_LOCAL_API_DIAGNOSTICS_COMMAND);
  if (!isRawLocalApiDiagnostics(payload)) {
    throw new Error("Invalid local API diagnostics payload");
  }

  return mapRawLocalApiDiagnostics(payload);
}

export async function getLocalApiSettings(): Promise<LocalApiSettingsSnapshot> {
  const payload = await invoke<unknown>(GET_LOCAL_API_SETTINGS_COMMAND);
  if (!isRawLocalApiSettings(payload)) {
    throw new Error("Invalid local API settings payload");
  }

  return mapRawLocalApiSettings(payload);
}

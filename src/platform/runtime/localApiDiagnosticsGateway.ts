import { invoke } from "@tauri-apps/api/core";

const GET_LOCAL_API_DIAGNOSTICS_COMMAND = "cmd_get_local_api_diagnostics";

interface RawLocalApiDiagnosticsSnapshot {
  base_url: string;
  token_path: string;
  token_present: boolean;
  listening: boolean;
}

export interface LocalApiDiagnosticsSnapshot {
  baseUrl: string;
  tokenPath: string;
  tokenPresent: boolean;
  listening: boolean;
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

function mapRawLocalApiDiagnostics(raw: RawLocalApiDiagnosticsSnapshot): LocalApiDiagnosticsSnapshot {
  return {
    baseUrl: raw.base_url,
    tokenPath: raw.token_path,
    tokenPresent: raw.token_present,
    listening: raw.listening,
  };
}

export async function getLocalApiDiagnostics(): Promise<LocalApiDiagnosticsSnapshot> {
  const payload = await invoke<unknown>(GET_LOCAL_API_DIAGNOSTICS_COMMAND);
  if (!isRawLocalApiDiagnostics(payload)) {
    throw new Error("Invalid local API diagnostics payload");
  }

  return mapRawLocalApiDiagnostics(payload);
}

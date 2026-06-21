import type { WebActivityBridgeSnapshot } from "../../../platform/runtime/webActivityBridgeGateway.ts";
import type { LocalApiDiagnosticsSnapshot } from "../../../platform/runtime/localApiDiagnosticsGateway.ts";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking.ts";
import { resolvePlatformTrackingDiagnosticMessage } from "../../../app/services/platformTrackingDiagnosticsService.ts";

export type SettingsDiagnosticTone = "ok" | "warning" | "muted";

export interface SettingsDiagnosticItem {
  id: string;
  label: string;
  value: string;
  detail: string;
  tone: SettingsDiagnosticTone;
}

export interface SettingsDiagnosticsInput {
  trackerHealth: TrackerHealthSnapshot;
  webActivityEnabled: boolean;
  webActivityPort: number;
  webActivityToken: string;
  webActivityBridge: WebActivityBridgeSnapshot | null;
  localApi?: LocalApiDiagnosticsSnapshot | null;
  apiBaseUrl?: string;
  apiTokenPath?: string;
}

export function buildSettingsDiagnosticsViewModel(
  input: SettingsDiagnosticsInput,
): SettingsDiagnosticItem[] {
  const apiBaseUrl = input.apiBaseUrl ?? "http://127.0.0.1:14840";
  const apiTokenPath = input.apiTokenPath ?? "${XDG_DATA_HOME:-~/.local/share}/Patina/api_token";
  const windowTracking = input.trackerHealth.platformDiagnostics?.windowTracking;
  const platformMessage = resolvePlatformTrackingDiagnosticMessage(input.trackerHealth.platformDiagnostics);

  return [
    {
      id: "window-tracking",
      label: "窗口追踪",
      value: resolveWindowTrackingValue(input.trackerHealth),
      detail: platformMessage
        ?? resolveWindowTrackingDetail(windowTracking?.provider, windowTracking?.sessionType, windowTracking?.desktop),
      tone: input.trackerHealth.status === "healthy" && !platformMessage ? "ok" : "warning",
    },
    {
      id: "local-api",
      label: "本地 API",
      value: resolveLocalApiValue(input.localApi, apiBaseUrl),
      detail: resolveLocalApiDetail(input.localApi, apiTokenPath),
      tone: resolveLocalApiTone(input.localApi),
    },
    {
      id: "browser-bridge",
      label: "浏览器扩展",
      value: resolveBrowserBridgeValue(input.webActivityEnabled, input.webActivityBridge),
      detail: resolveBrowserBridgeDetail(input.webActivityEnabled, input.webActivityToken, input.webActivityBridge, input.webActivityPort),
      tone: resolveBrowserBridgeTone(input.webActivityEnabled, input.webActivityToken, input.webActivityBridge),
    },
  ];
}

function resolveLocalApiValue(
  localApi: LocalApiDiagnosticsSnapshot | null | undefined,
  fallbackBaseUrl: string,
): string {
  if (!localApi) return fallbackBaseUrl;
  if (!localApi.tokenPresent) return "Token 缺失";
  if (!localApi.listening) return "未监听";
  return "已监听";
}

function resolveLocalApiDetail(
  localApi: LocalApiDiagnosticsSnapshot | null | undefined,
  fallbackTokenPath: string,
): string {
  if (!localApi) return `Token: ${fallbackTokenPath}`;

  const status = localApi.listening ? "可连接" : "不可连接";
  const token = localApi.tokenPresent ? "Token 已生成" : "Token 未生成";
  return `${localApi.baseUrl} / ${status} / ${token} / ${localApi.tokenPath}`;
}

function resolveLocalApiTone(
  localApi: LocalApiDiagnosticsSnapshot | null | undefined,
): SettingsDiagnosticTone {
  if (!localApi) return "muted";
  if (!localApi.tokenPresent || !localApi.listening) return "warning";
  return "ok";
}

function resolveWindowTrackingValue(trackerHealth: TrackerHealthSnapshot): string {
  if (trackerHealth.status !== "healthy") {
    return "追踪运行时未就绪";
  }

  const status = trackerHealth.platformDiagnostics?.windowTracking.status;
  if (status === "available") return "可用";
  if (status === "unsupported") return "暂不支持";
  if (status === "unavailable") return "不可用";
  return "运行中";
}

function resolveWindowTrackingDetail(
  provider: string | undefined,
  sessionType: string | null | undefined,
  desktop: string | null | undefined,
): string {
  const parts = [
    provider ? `Provider: ${provider}` : null,
    sessionType ? `Session: ${sessionType}` : null,
    desktop ? `Desktop: ${desktop}` : null,
  ].filter(Boolean);
  return parts.length > 0 ? parts.join(" / ") : "平台诊断信息暂不可用。";
}

function resolveBrowserBridgeValue(
  webActivityEnabled: boolean,
  bridge: WebActivityBridgeSnapshot | null,
): string {
  if (!webActivityEnabled) return "未启用";
  if (!bridge) return "状态未知";
  if (bridge.connected) return "已连接";
  return "未连接";
}

function resolveBrowserBridgeDetail(
  webActivityEnabled: boolean,
  webActivityToken: string,
  bridge: WebActivityBridgeSnapshot | null,
  port: number,
): string {
  if (!webActivityEnabled) {
    return "网页同步关闭时不会记录 URL 和域名。";
  }
  if (webActivityToken.trim().length === 0) {
    return "缺少浏览器扩展 Token，启用前需要生成 Token。";
  }
  if (!bridge) {
    return `等待浏览器扩展连接本地端口 ${port}。`;
  }
  if (!bridge.connected) {
    return `未收到最近的浏览器扩展上报。端口: ${port}`;
  }

  const browser = bridge.browserKind ?? "browser";
  const version = bridge.extensionVersion ? ` / v${bridge.extensionVersion}` : "";
  const lastSeen = bridge.lastActivityAtMs ? ` / last ${new Date(bridge.lastActivityAtMs).toLocaleTimeString()}` : "";
  return `${browser}${version}${lastSeen}`;
}

function resolveBrowserBridgeTone(
  webActivityEnabled: boolean,
  webActivityToken: string,
  bridge: WebActivityBridgeSnapshot | null,
): SettingsDiagnosticTone {
  if (!webActivityEnabled) return "muted";
  if (webActivityToken.trim().length === 0) return "warning";
  if (!bridge || !bridge.connected) return "warning";
  return "ok";
}

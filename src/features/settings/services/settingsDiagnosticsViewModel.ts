import type { WebActivityBridgeSnapshot } from "../../../platform/runtime/webActivityBridgeGateway.ts";
import type { LocalApiDiagnosticsSnapshot } from "../../../platform/runtime/localApiDiagnosticsGateway.ts";
import type { DesktopIntegrationDiagnosticsSnapshot } from "../../../platform/runtime/desktopIntegrationDiagnosticsGateway.ts";
import type { TrackerHealthSnapshot } from "../../../shared/types/tracking.ts";
import { resolvePlatformTrackingDiagnosticMessage } from "../../../app/services/platformTrackingDiagnosticsService.ts";

export type SettingsDiagnosticTone = "ok" | "warning" | "danger" | "muted";

export interface SettingsDiagnosticItem {
  id: string;
  label: string;
  value: string;
  detail: string;
  tone: SettingsDiagnosticTone;
  metadata?: SettingsDiagnosticMetadata[];
}

export interface SettingsDiagnosticMetadata {
  label: string;
  value: string;
}

export interface SettingsDiagnosticsInput {
  trackerHealth: TrackerHealthSnapshot;
  webActivityEnabled: boolean;
  webActivityPort: number;
  webActivityToken: string;
  webActivityBridge: WebActivityBridgeSnapshot | null;
  localApi?: LocalApiDiagnosticsSnapshot | null;
  desktopIntegration?: DesktopIntegrationDiagnosticsSnapshot | null;
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
      tone: input.trackerHealth.status === "healthy" && !platformMessage ? "ok" : "danger",
    },
    {
      id: "local-api",
      label: "本地 API",
      value: resolveLocalApiValue(input.localApi, apiBaseUrl),
      detail: resolveLocalApiDetail(input.localApi, apiTokenPath),
      tone: resolveLocalApiTone(input.localApi),
      metadata: resolveLocalApiMetadata(input.localApi, apiBaseUrl, apiTokenPath),
    },
    {
      id: "desktop-integration",
      label: "桌面集成",
      value: resolveDesktopIntegrationValue(input.desktopIntegration),
      detail: resolveDesktopIntegrationDetail(input.desktopIntegration),
      tone: resolveDesktopIntegrationTone(input.desktopIntegration),
      metadata: resolveDesktopIntegrationMetadata(input.desktopIntegration),
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

function resolveLocalApiMetadata(
  localApi: LocalApiDiagnosticsSnapshot | null | undefined,
  fallbackBaseUrl: string,
  fallbackTokenPath: string,
): SettingsDiagnosticMetadata[] {
  return [
    { label: "Base URL", value: localApi?.baseUrl ?? fallbackBaseUrl },
    { label: "Token file", value: localApi?.tokenPath ?? fallbackTokenPath },
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
  return `${status} / ${token}`;
}

function resolveLocalApiTone(
  localApi: LocalApiDiagnosticsSnapshot | null | undefined,
): SettingsDiagnosticTone {
  if (!localApi) return "muted";
  if (!localApi.tokenPresent || !localApi.listening) return "danger";
  return "ok";
}

function resolveDesktopIntegrationValue(
  desktopIntegration: DesktopIntegrationDiagnosticsSnapshot | null | undefined,
): string {
  if (!desktopIntegration) return "状态未知";
  if (!desktopIntegration.launchAtLogin) return "未启用";
  if (!desktopIntegration.autostart.exists) return "未写入";
  if (!desktopIntegration.autostart.valid) return "自启动异常";
  return desktopIntegration.startMinimized ? "自启动 / 最小化" : "自启动";
}

function resolveDesktopIntegrationDetail(
  desktopIntegration: DesktopIntegrationDiagnosticsSnapshot | null | undefined,
): string {
  if (!desktopIntegration) {
    return "等待读取桌面启动项状态。";
  }

  if (!desktopIntegration.launchAtLogin) {
    return "登录自启动关闭时，不会检查启动项是否可用。";
  }

  const exec = desktopIntegration.autostart.exec;
  if (!desktopIntegration.autostart.exists) {
    return `未找到自启动文件：${desktopIntegration.autostart.path}`;
  }
  if (!exec || exec.trim().length === 0) {
    return `自启动文件缺少 Exec：${desktopIntegration.autostart.path}`;
  }
  if (!desktopIntegration.autostart.valid) {
    return `Exec 当前为 ${exec}，需要指向 Patina 并包含 --autostart。`;
  }

  return `自启动文件有效：${desktopIntegration.autostart.path}`;
}

function resolveDesktopIntegrationTone(
  desktopIntegration: DesktopIntegrationDiagnosticsSnapshot | null | undefined,
): SettingsDiagnosticTone {
  if (!desktopIntegration || !desktopIntegration.launchAtLogin) return "muted";
  if (!desktopIntegration.autostart.valid) return "danger";
  return "ok";
}

function resolveDesktopIntegrationMetadata(
  desktopIntegration: DesktopIntegrationDiagnosticsSnapshot | null | undefined,
): SettingsDiagnosticMetadata[] {
  if (!desktopIntegration) return [];

  return [
    { label: "Autostart", value: desktopIntegration.autostart.path },
    { label: "Exec", value: desktopIntegration.autostart.exec ?? "未设置" },
  ];
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
  if (webActivityToken.trim().length === 0) return "danger";
  if (!bridge || !bridge.connected) return "danger";
  return "ok";
}

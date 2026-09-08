import type { WebActivityBridgeSnapshot } from "../../../platform/runtime/webActivityBridgeGateway.ts";
import type { LocalApiDiagnosticsSnapshot } from "../../../platform/runtime/localApiDiagnosticsGateway.ts";
import type { DesktopIntegrationDiagnosticsSnapshot } from "../../../platform/runtime/desktopIntegrationDiagnosticsGateway.ts";
import type { DaemonServiceDiagnosticsSnapshot } from "../../../platform/runtime/daemonServiceDiagnosticsGateway.ts";
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
  daemonService?: DaemonServiceDiagnosticsSnapshot | null;
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
      id: "daemon-service",
      label: "后台服务",
      value: resolveDaemonServiceValue(input.daemonService),
      detail: resolveDaemonServiceDetail(input.daemonService),
      tone: resolveDaemonServiceTone(input.daemonService),
      metadata: resolveDaemonServiceMetadata(input.daemonService),
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

function resolveDaemonServiceValue(
  daemonService: DaemonServiceDiagnosticsSnapshot | null | undefined,
): string {
  if (!daemonService) return "状态未知";
  if (!daemonService.managerAvailable) return "systemd 不可用";
  if (!daemonService.unitInstalled) return "未安装";
  if (daemonService.migrationState === "owner-conflict") return "运行冲突";
  if (daemonService.migrationState === "cutover-failed") return "接管失败";
  if (daemonService.migrationState === "cutover-pending") return "接管中";
  if (daemonService.migrationState === "preference-mismatch") return "设置待对账";
  if (daemonService.active) return "运行中";
  return "已安装 / 未启用";
}

function resolveDaemonServiceDetail(
  daemonService: DaemonServiceDiagnosticsSnapshot | null | undefined,
): string {
  if (!daemonService) return "等待读取 patinad 服务状态。";
  if (daemonService.error) return daemonService.error;
  if (!daemonService.managerAvailable) return "无法连接当前用户的 systemd manager。";
  if (!daemonService.unitInstalled) return "当前安装未包含 patinad.service；daemon-backed DEB 才会安装该 unit。";
  if (daemonService.migrationState === "owner-conflict") {
    return "服务已启用或运行，但当前版本仍由 Patina Desktop 追踪。请先停用 patinad.service，避免两个追踪进程竞争。";
  }
  if (daemonService.migrationState === "cutover-failed") {
    const failure = daemonService.cutover.failureMessage ?? "未提供具体错误";
    return `后台接管未完成：${failure}。Patina 已暂停自动回退，避免同时启动两个追踪进程。`;
  }
  if (daemonService.migrationState === "cutover-pending") {
    return "正在等待桌面端重启或后台服务完成追踪就绪确认。";
  }
  if (daemonService.migrationState === "managed") {
    return "后台追踪由 patinad.service 持续运行，关闭桌面窗口不会停止记录。";
  }
  if (daemonService.migrationState === "managed-blocked") {
    return "桌面端已切换为后台服务客户端，但 patinad.service 未运行；追踪当前处于暂停状态，需要修复或重试后台服务。";
  }
  if (daemonService.migrationState === "preference-mismatch") {
    return "后台登录偏好尚未与 patinad.service 状态一致；请重试该设置或重新启动 Patina 完成对账。";
  }
  if (daemonService.migrationState === "ready") {
    return "服务按计划保持禁用；现有桌面自启动满足后续安全迁移条件。";
  }
  if (daemonService.migrationState === "blocked") {
    return "服务保持禁用；需要先修复桌面启动项，之后才能迁移后台追踪。";
  }
  return "服务按计划保持禁用，当前追踪仍由 Patina Desktop 负责。";
}

function resolveDaemonServiceTone(
  daemonService: DaemonServiceDiagnosticsSnapshot | null | undefined,
): SettingsDiagnosticTone {
  if (!daemonService) return "muted";
  if (daemonService.error || !daemonService.managerAvailable) return "danger";
  if (daemonService.migrationState === "owner-conflict") return "danger";
  if (daemonService.migrationState === "cutover-failed") return "danger";
  if (daemonService.migrationState === "cutover-pending") return "warning";
  if (daemonService.migrationState === "managed") return "ok";
  if (daemonService.migrationState === "managed-blocked") return "danger";
  if (daemonService.migrationState === "preference-mismatch") return "warning";
  if (!daemonService.unitInstalled) return "warning";
  if (daemonService.migrationState === "blocked") return "warning";
  return "ok";
}

function resolveDaemonServiceMetadata(
  daemonService: DaemonServiceDiagnosticsSnapshot | null | undefined,
): SettingsDiagnosticMetadata[] {
  if (!daemonService) return [];

  const metadata = [
    { label: "Unit", value: daemonService.serviceName },
    { label: "Unit file", value: daemonService.unitFileState ?? "未找到" },
    {
      label: "Runtime",
      value: [daemonService.activeState, daemonService.subState].filter(Boolean).join(" / ") || "未加载",
    },
  ];
  if (daemonService.cutover.state !== "not-requested" && daemonService.cutover.state !== "unsupported") {
    metadata.push({ label: "Cutover", value: daemonService.cutover.state });
  }
  if (daemonService.cutover.failureCode) {
    metadata.push({ label: "Failure", value: daemonService.cutover.failureCode });
  }
  return metadata;
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
  if (!bridge.listening) return "监听异常";
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
  if (!bridge.listening) {
    return `本地桥接端口 ${port} 未成功监听，请检查端口占用或后台运行状态。`;
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
  if (!bridge || !bridge.listening || !bridge.connected) return "danger";
  return "ok";
}

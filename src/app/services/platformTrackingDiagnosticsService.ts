import type { PlatformTrackingDiagnostics } from "../../shared/types/tracking.ts";

const WINDOW_TRACKING_MESSAGES: Record<string, string> = {
  "gnome-extension-dbus-unavailable": "GNOME 扩展 D-Bus 不可用，当前无法可靠读取焦点窗口。",
  "session-bus-unavailable": "Session D-Bus 不可用，当前无法读取 Linux 焦点窗口。",
  "wayland-compositor-unsupported": "当前 Wayland 桌面暂未适配窗口追踪。",
  "unknown-session-type": "无法识别当前 Linux 会话类型，窗口追踪可能不可用。",
  "platform-unsupported": "当前平台暂不支持窗口追踪。",
};

export function resolvePlatformTrackingDiagnosticMessage(
  diagnostics: PlatformTrackingDiagnostics | undefined,
): string | null {
  const windowTracking = diagnostics?.windowTracking;
  if (!windowTracking || windowTracking.status === "available") {
    return null;
  }

  if (windowTracking.reason && WINDOW_TRACKING_MESSAGES[windowTracking.reason]) {
    return WINDOW_TRACKING_MESSAGES[windowTracking.reason];
  }

  return windowTracking.status === "unsupported"
    ? "当前平台暂不支持窗口追踪。"
    : "窗口追踪当前不可用。";
}

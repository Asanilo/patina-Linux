import {
  getWebActivityBridgeSnapshot,
  type WebActivityBridgeSnapshot,
} from "../../../platform/runtime/webActivityBridgeGateway.ts";
import {
  getLocalApiDiagnostics,
  type LocalApiDiagnosticsSnapshot,
} from "../../../platform/runtime/localApiDiagnosticsGateway.ts";
import {
  getDesktopIntegrationDiagnostics,
  repairAutostartDesktopFile,
  type DesktopIntegrationDiagnosticsSnapshot,
} from "../../../platform/runtime/desktopIntegrationDiagnosticsGateway.ts";
import {
  getDaemonServiceDiagnostics,
  reloadDaemonVersion,
  retryRuntimeOwnerCutover,
  rollbackRuntimeOwnerToEmbedded,
  setBackgroundTrackingAtLogin,
  type DaemonServiceDiagnosticsSnapshot,
} from "../../../platform/runtime/daemonServiceDiagnosticsGateway.ts";

export type {
  WebActivityBridgeSnapshot,
  LocalApiDiagnosticsSnapshot,
  DesktopIntegrationDiagnosticsSnapshot,
  DaemonServiceDiagnosticsSnapshot,
};

const defaultDeps = {
  getWebActivityBridgeSnapshot,
  getLocalApiDiagnostics,
  getDesktopIntegrationDiagnostics,
  getDaemonServiceDiagnostics,
  repairAutostartDesktopFile,
  reloadDaemonVersion,
  retryRuntimeOwnerCutover,
  rollbackRuntimeOwnerToEmbedded,
  setBackgroundTrackingAtLogin,
  reportWarning: (message: string, error: unknown) => console.warn(message, error),
};

function snapshotOrNull<T>(
  result: PromiseSettledResult<T>,
  message: string,
  reportWarning: typeof defaultDeps.reportWarning,
): T | null {
  if (result.status === "fulfilled") return result.value;
  reportWarning(message, result.reason);
  return null;
}

export function createSettingsDiagnosticsService(deps = defaultDeps) {
  return {
    // A failure in one diagnostic must not hide healthy, independently loaded rows.
    async loadLive() {
      const [bridge, localApi, desktopIntegration] = await Promise.allSettled([
        deps.getWebActivityBridgeSnapshot(),
        deps.getLocalApiDiagnostics(),
        deps.getDesktopIntegrationDiagnostics(),
      ]);
      return {
        bridge: snapshotOrNull(bridge, "load web activity bridge snapshot failed", deps.reportWarning),
        localApi: snapshotOrNull(localApi, "load local API diagnostics failed", deps.reportWarning),
        desktopIntegration: snapshotOrNull(
          desktopIntegration,
          "load desktop integration diagnostics failed",
          deps.reportWarning,
        ),
      };
    },
    loadDaemon: deps.getDaemonServiceDiagnostics,
    repairAutostart: deps.repairAutostartDesktopFile,
    reloadDaemon: deps.reloadDaemonVersion,
    retryCutover: deps.retryRuntimeOwnerCutover,
    rollbackOwner: deps.rollbackRuntimeOwnerToEmbedded,
    setBackgroundTrackingAtLogin: deps.setBackgroundTrackingAtLogin,
  };
}

export const SettingsDiagnosticsService = createSettingsDiagnosticsService();

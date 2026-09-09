import type {
  DaemonServiceDiagnosticsSnapshot,
  RuntimeOwnerCutoverState,
} from "../../../platform/runtime/daemonServiceDiagnosticsGateway.ts";

export interface DaemonServiceControlAvailability {
  backgroundLogin: boolean;
  retry: boolean;
  rollback: boolean;
}

const RETRY_STATES = new Set<RuntimeOwnerCutoverState>(["failed", "blocked", "rolled-back"]);

export function canReloadDaemonVersion(snapshot: DaemonServiceDiagnosticsSnapshot | null): boolean {
  const version = snapshot?.version;
  return Boolean(snapshot?.controlAvailable && snapshot.active && snapshot.cutover.state === "completed"
    && version?.restartAvailable && !version.error && version.runningVersion
    && version.runningVersion !== version.desktopVersion);
}
const ROLLBACK_STATES = new Set<RuntimeOwnerCutoverState>([
  "completed",
  "failed",
  "blocked",
  "rolling-back",
]);

export function resolveDaemonServiceControlAvailability(
  snapshot: DaemonServiceDiagnosticsSnapshot | null,
): DaemonServiceControlAvailability {
  if (!snapshot?.controlAvailable) {
    return {
      backgroundLogin: false,
      retry: false,
      rollback: false,
    };
  }

  const state = snapshot.cutover.state;
  return {
    backgroundLogin: state === "completed",
    retry: RETRY_STATES.has(state),
    rollback: ROLLBACK_STATES.has(state),
  };
}

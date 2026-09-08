import type {
  DaemonServiceDiagnosticsSnapshot,
  RuntimeOwnerCutoverState,
} from "../../../platform/runtime/daemonServiceDiagnosticsGateway.ts";

export interface DaemonServiceControlAvailability {
  backgroundLogin: boolean;
  retry: boolean;
  rollback: boolean;
}

const RETRY_STATES = new Set<RuntimeOwnerCutoverState>(["failed", "blocked"]);
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

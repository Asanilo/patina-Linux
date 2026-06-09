import type { ToolsRuntimeSnapshot } from "../../../shared/types/tools.ts";
import { ToolsRuntimeService } from "./toolsRuntimeService.ts";

type ToolsRuntimeSnapshotListener = (snapshot: ToolsRuntimeSnapshot) => void;

interface ToolsRuntimeSnapshotStoreDeps {
  getSnapshot: () => Promise<ToolsRuntimeSnapshot>;
  onChanged: (listener: ToolsRuntimeSnapshotListener) => Promise<() => void>;
  warn: (message: string, error: unknown) => void;
}

export interface ToolsRuntimeSnapshotStore {
  getCurrentSnapshot: () => ToolsRuntimeSnapshot | null;
  publishSnapshot: (snapshot: ToolsRuntimeSnapshot) => void;
  refreshSnapshot: () => Promise<ToolsRuntimeSnapshot>;
  subscribe: (listener: ToolsRuntimeSnapshotListener) => () => void;
}

export function createToolsRuntimeSnapshotStore(
  deps: ToolsRuntimeSnapshotStoreDeps,
): ToolsRuntimeSnapshotStore {
  const listeners = new Set<ToolsRuntimeSnapshotListener>();
  let currentSnapshot: ToolsRuntimeSnapshot | null = null;
  let runtimeUnlisten: (() => void) | null = null;
  let pendingRuntimeListen: Promise<void> | null = null;
  let pendingRefresh: Promise<ToolsRuntimeSnapshot> | null = null;

  const publishSnapshot = (snapshot: ToolsRuntimeSnapshot) => {
    currentSnapshot = snapshot;
    for (const listener of listeners) {
      listener(snapshot);
    }
  };

  const detachRuntimeListenerIfUnused = () => {
    if (listeners.size > 0 || !runtimeUnlisten) return;

    const dispose = runtimeUnlisten;
    runtimeUnlisten = null;
    dispose();
  };

  const ensureRuntimeListener = () => {
    if (runtimeUnlisten || pendingRuntimeListen) return;

    pendingRuntimeListen = deps.onChanged(publishSnapshot)
      .then((dispose) => {
        pendingRuntimeListen = null;
        if (listeners.size === 0) {
          dispose();
          return;
        }

        runtimeUnlisten = dispose;
      })
      .catch((error) => {
        pendingRuntimeListen = null;
        deps.warn("listen tools runtime snapshot failed", error);
      });
  };

  return {
    getCurrentSnapshot: () => currentSnapshot,
    publishSnapshot,
    refreshSnapshot() {
      if (pendingRefresh) {
        return pendingRefresh;
      }

      pendingRefresh = deps.getSnapshot()
        .then((snapshot) => {
          publishSnapshot(snapshot);
          return snapshot;
        })
        .finally(() => {
          pendingRefresh = null;
        });

      return pendingRefresh;
    },
    subscribe(listener) {
      listeners.add(listener);
      if (currentSnapshot) {
        listener(currentSnapshot);
      }
      ensureRuntimeListener();

      return () => {
        listeners.delete(listener);
        detachRuntimeListenerIfUnused();
      };
    },
  };
}

export const toolsRuntimeSnapshotStore = createToolsRuntimeSnapshotStore({
  getSnapshot: ToolsRuntimeService.getToolsSnapshot,
  onChanged: ToolsRuntimeService.onToolsRuntimeChanged,
  warn: (message, error) => console.warn(message, error),
});

export function prewarmToolsRuntimeSnapshot(): Promise<ToolsRuntimeSnapshot> {
  return toolsRuntimeSnapshotStore.refreshSnapshot();
}
